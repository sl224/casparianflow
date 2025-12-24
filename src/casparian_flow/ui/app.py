import logging
from pathlib import Path
from fasthtml.common import *
from sqlalchemy.orm import Session

from casparian_flow.config import settings
from casparian_flow.db.access import get_engine
from casparian_flow.db.base_session import Base
# Import ALL models to register them with Base.metadata
from casparian_flow.db.models import (
    FileLocation, FileTag, PluginConfig, PluginSubscription,
    RoutingRule, IgnoreRule, TopicConfig, SourceRoot,
    FileHashRegistry, FileVersion, ProcessingJob, WorkerNode,
    PluginManifest, LibraryWhitelist, SurveyorSession, SurveyorDecision
)
from casparian_flow.engine.job_queue import JobQueue
from casparian_flow.server.api import signal_sentinel_reload

# Setup Logging
logger = logging.getLogger(__name__)

# Initialize FastHTML app with Pico.css
app = FastHTML(
    hdrs=(
        Link(rel="stylesheet", href="https://unpkg.com/@picocss/pico@2/css/pico.min.css"),
        Style("""
            nav { padding: 1rem; background: var(--pico-card-background-color); margin-bottom: 2rem; }
            nav ul { display: flex; gap: 2rem; list-style: none; margin: 0; padding: 0; }
            .tag { display: inline-block; padding: 0.2rem 0.5rem; margin: 0.1rem; 
                   background: var(--pico-primary); color: white; border-radius: 4px; font-size: 0.8rem; }
            .toast { padding: 1rem; background: var(--pico-ins-color); border-radius: 8px; margin: 1rem 0; }
            .status-active { color: var(--pico-ins-color); }
            .status-inactive { color: var(--pico-del-color); }
        """)
    )
)

# Database Engine with absolute path logging
engine = get_engine(settings.database)

# Log the absolute database path for debugging
if settings.database.type == "sqlite3":
    if settings.database.in_memory:
        logger.info("Using in-memory SQLite database")
    else:
        db_path = Path(settings.database.db_location).resolve()
        logger.info(f"SQLite database path: {db_path}")
        logger.info(f"Database exists: {db_path.exists()}")

# Ensure tables exist
logger.info("Creating database tables if they don't exist...")
Base.metadata.create_all(engine)

# Verify tables were created
from sqlalchemy import inspect
inspector = inspect(engine)
table_names = inspector.get_table_names()
logger.info(f"Available tables: {table_names}")
if not table_names:
    logger.error("No tables found after create_all()! Check model imports and schema configuration.")

# Database session generator
def get_db():
    """Yield database sessions for routes."""
    db = Session(engine)
    try:
        yield db
    finally:
        db.close()

# Layout Function
def layout(title: str, *content):
    return Html(
        Head(
            Title(f"Casparian Flow - {title}"),
            Meta(charset="utf-8"),
            Meta(name="viewport", content="width=device-width, initial-scale=1"),
        ),
        Body(
            Nav(
                Ul(
                    Li(A("🏠 Home", href="/")),
                    Li(A("📁 Inventory", href="/inventory")),
                    Li(A("📥 Import", href="/import")),
                    Li(A("🔌 Wiring", href="/wiring")),
                    Li(A("⚙️ Operations", href="/operations")),
                ),
            ),
            Main(
                *content,
                cls="container"
            ),
        )
    )

# --- Routes ---

@app.get("/")
def home():
    return layout("Dashboard",
        H1("Casparian Flow"),
        P("Glass Box UI for system management."),
        Div(
            Article(
                Header("Quick Stats"),
                P("Navigate using the menu above to manage files, configure wiring, or run jobs."),
                Footer(
                    A("📁 Browse Files", href="/inventory", role="button"),
                    A("🔌 Configure Wiring", href="/wiring", role="button", cls="secondary"),
                    A("⚙️ Run Jobs", href="/operations", role="button", cls="contrast"),
                ),
            ),
        ),
    )

# --- Inventory Page ---

@app.get("/inventory")
def inventory():
    db = next(get_db())
    try:
        files = db.query(FileLocation).order_by(FileLocation.id.desc()).limit(50).all()

        if not files:
            return layout("Inventory",
                H1("📁 File Inventory"),
                P("No files found in the inventory."),
                Article(
                    "The inventory is empty. Files will appear here once they are scanned and added to the system.",
                    style="background: var(--pico-card-background-color); padding: 2rem;"
                ),
            )

        rows = []
        for f in files:
            # Get tags for this file
            tags = db.query(FileTag.tag).filter(FileTag.file_id == f.id).all()
            tag_list = [t[0] for t in tags]

            rows.append(Tr(
                Td(str(f.id)),
                Td(f.filename or "N/A"),
                Td(f.rel_path or "N/A"),
                Td(
                    # Existing tags
                    Span(*[Span(t, cls="tag") for t in tag_list]),
                    # Inline add form
                    Form(
                        Input(type="text", name="tag", placeholder="Add tag...",
                              style="width: 100px; display: inline; margin-left: 0.5rem;"),
                        Button("+", type="submit", style="padding: 0.2rem 0.5rem;"),
                        hx_post=f"/inventory/tag/{f.id}",
                        hx_target=f"#tags-{f.id}",
                        hx_swap="innerHTML",
                        style="display: inline;"
                    ),
                    id=f"tags-{f.id}"
                ),
            ))

        return layout("Inventory",
            H1("📁 File Inventory"),
            P(f"Showing {len(files)} most recent files."),
            Table(
                Thead(Tr(Th("ID"), Th("Filename"), Th("Path"), Th("Tags"))),
                Tbody(*rows),
            ),
        )
    except Exception as e:
        logger.error(f"Error loading inventory: {e}", exc_info=True)
        return layout("Inventory",
            H1("📁 File Inventory"),
            Article(
                f"❌ Error loading inventory: {str(e)}",
                style="background: var(--pico-del-color); padding: 2rem;"
            ),
        )
    finally:
        db.close()

@app.post("/inventory/tag/{file_id}")
def add_tag(file_id: int, tag: str):
    db = next(get_db())
    try:
        # Add tag if not exists
        existing = db.query(FileTag).filter_by(file_id=file_id, tag=tag).first()
        if not existing and tag.strip():
            db.add(FileTag(file_id=file_id, tag=tag.strip()))
            db.commit()

        # Return updated tag list
        tags = db.query(FileTag.tag).filter(FileTag.file_id == file_id).all()
        tag_list = [t[0] for t in tags]

        return Div(
            Span(*[Span(t, cls="tag") for t in tag_list]),
            Form(
                Input(type="text", name="tag", placeholder="Add tag...",
                      style="width: 100px; display: inline; margin-left: 0.5rem;"),
                Button("+", type="submit", style="padding: 0.2rem 0.5rem;"),
                hx_post=f"/inventory/tag/{file_id}",
                hx_target=f"#tags-{file_id}",
                hx_swap="innerHTML",
                style="display: inline;"
            ),
        )
    except Exception as e:
        logger.error(f"Error adding tag to file {file_id}: {e}", exc_info=True)
        return Div(f"❌ Error: {str(e)}", style="color: var(--pico-del-color);")
    finally:
        db.close()

# --- Wiring Page ---

@app.get("/wiring")
def wiring():
    db = next(get_db())
    try:
        # Get all plugins
        plugins = db.query(PluginConfig).all()

        # Get all subscriptions
        subs = db.query(PluginSubscription).all()
        sub_map = {}
        for s in subs:
            key = s.plugin_name
            if key not in sub_map:
                sub_map[key] = []
            sub_map[key].append(s)

        rows = []
        for plugin in plugins:
            plugin_subs = sub_map.get(plugin.plugin_name, [])

            for sub in plugin_subs:
                rows.append(Tr(
                    Td(plugin.plugin_name),
                    Td(sub.topic_name),
                    Td(
                        Form(
                            Input(type="checkbox", name="is_active",
                                  checked=sub.is_active,
                                  hx_put=f"/wiring/{sub.id}",
                                  hx_target=f"#sub-status-{sub.id}",
                                  hx_swap="innerHTML",
                            ),
                            id=f"sub-status-{sub.id}"
                        ),
                        cls="status-active" if sub.is_active else "status-inactive"
                    ),
                ))

            # If no subscriptions, show the plugin anyway
            if not plugin_subs:
                rows.append(Tr(
                    Td(plugin.plugin_name),
                    Td("-"),
                    Td("-"),
                ))

        return layout("Wiring",
            H1("🔌 Plugin Wiring"),
            P("Configure which plugins subscribe to which topics."),
            Table(
                Thead(Tr(Th("Plugin"), Th("Topic"), Th("Active"))),
                Tbody(*rows if rows else [Tr(Td("No plugins configured", colspan="3"))]),
            ),
            Hr(),
            Article(
                Header("Add Subscription"),
                Form(
                    Fieldset(
                        Label("Plugin Name",
                              Input(type="text", name="plugin_name", placeholder="my_plugin")),
                        Label("Topic",
                              Input(type="text", name="topic", placeholder="raw_data")),
                    ),
                    Button("Add Subscription", type="submit"),
                    hx_post="/wiring/add",
                    hx_target="#wiring-result",
                    hx_swap="innerHTML",
                ),
                Div(id="wiring-result"),
            ),
        )
    except Exception as e:
        logger.error(f"Error loading wiring page: {e}", exc_info=True)
        return layout("Wiring",
            H1("🔌 Plugin Wiring"),
            Article(
                f"❌ Error loading wiring configuration: {str(e)}",
                style="background: var(--pico-del-color); padding: 2rem;"
            ),
        )
    finally:
        db.close()

@app.put("/wiring/{sub_id}")
def toggle_subscription(sub_id: int, is_active: bool = False):
    db = next(get_db())
    try:
        sub = db.get(PluginSubscription, sub_id)
        if sub:
            sub.is_active = is_active
            db.commit()

            # Hot Reload Sentinel
            signal_sentinel_reload()

        return Input(type="checkbox", name="is_active",
                     checked=is_active,
                     hx_put=f"/wiring/{sub_id}",
                     hx_target=f"#sub-status-{sub_id}",
                     hx_swap="innerHTML",
        )
    except Exception as e:
        logger.error(f"Error toggling subscription {sub_id}: {e}", exc_info=True)
        return Div(f"❌ Error: {str(e)}", style="color: var(--pico-del-color);")
    finally:
        db.close()

@app.post("/wiring/add")
def add_subscription(plugin_name: str, topic: str):
    db = next(get_db())
    try:
        # Check plugin exists
        plugin = db.get(PluginConfig, plugin_name)
        if not plugin:
            return Div(f"❌ Plugin '{plugin_name}' not found", cls="toast", style="background: var(--pico-del-color);")

        # Add subscription
        existing = db.query(PluginSubscription).filter_by(
            plugin_name=plugin_name, topic_name=topic
        ).first()

        if existing:
            return Div(f"⚠️ Subscription already exists", cls="toast", style="background: var(--pico-mark-color);")

        db.add(PluginSubscription(plugin_name=plugin_name, topic_name=topic, is_active=True))
        db.commit()

        # Signal Sentinel
        signal_sentinel_reload()

        return Div(f"✅ Added subscription: {topic} → {plugin_name}", cls="toast")
    except Exception as e:
        logger.error(f"Error adding subscription {plugin_name} -> {topic}: {e}", exc_info=True)
        return Div(f"❌ Error: {str(e)}", cls="toast", style="background: var(--pico-del-color);")
    finally:
        db.close()

# --- Operations Page ---

@app.get("/operations")
def operations():
    db = next(get_db())
    try:
        plugins = db.query(PluginConfig).all()

        plugin_options = [Option(p.plugin_name, value=p.plugin_name) for p in plugins]

        return layout("Operations",
            H1("⚙️ Job Operations"),
            P("Manually submit processing jobs."),
            Article(
                Header("Submit Job"),
                Form(
                    Fieldset(
                        Label("File ID",
                              Input(type="number", name="file_id", placeholder="123", required=True)),
                        Label("Plugin",
                              Select(*plugin_options, name="plugin_name") if plugin_options else
                              Input(type="text", name="plugin_name", placeholder="plugin_name")),
                        Label("Priority (0-100)",
                              Input(type="number", name="priority", value="10", min="0", max="100")),
                    ),
                    Button("🚀 Run Job", type="submit"),
                    hx_post="/operations/submit",
                    hx_target="#job-result",
                    hx_swap="innerHTML",
                ),
                Div(id="job-result"),
            ),
        )
    except Exception as e:
        logger.error(f"Error loading operations page: {e}", exc_info=True)
        return layout("Operations",
            H1("⚙️ Job Operations"),
            Article(
                f"❌ Error loading operations page: {str(e)}",
                style="background: var(--pico-del-color); padding: 2rem;"
            ),
        )
    finally:
        db.close()

@app.post("/operations/submit")
def submit_job(file_id: int, plugin_name: str, priority: int = 10):
    try:
        engine = get_engine(settings.database)
        queue = JobQueue(engine)
        queue.push_job(file_id=file_id, plugin_name=plugin_name, priority=priority)
        
        return Div(
            f"✅ Job queued for file {file_id} with plugin '{plugin_name}'",
            cls="toast"
        )
    except Exception as e:
        return Div(
            f"❌ Failed to queue job: {str(e)}",
            cls="toast",
            style="background: var(--pico-del-color);"
        )

# --- Import Page ---

@app.get("/import")
def import_page():
    """Main import page with source selection."""
    db = next(get_db())
    try:
        source_roots = db.query(SourceRoot).filter(SourceRoot.active == 1).all()

        if not source_roots:
            return layout("Import Files",
                H1("📥 Import Files"),
                Article(
                    "No active source roots configured. Please configure source roots first.",
                    style="background: var(--pico-card-background-color); padding: 2rem;"
                )
            )

        return layout("Import Files",
            H1("📥 Import Files"),
            P("Import new files from existing source roots into managed storage."),
            Article(
                Header("Step 1: Select Source Root"),
                Form(
                    Select(
                        Option("-- Select a source root --", value="", selected=True, disabled=True),
                        *[Option(f"{sr.path} (ID: {sr.id})", value=sr.id) for sr in source_roots],
                        name="source_id",
                        hx_get="/import/browse-pane",
                        hx_target="#browse-pane",
                        hx_trigger="change",
                        hx_include="this"
                    ),
                ),
            ),
            Div(id="browse-pane"),
        )
    except Exception as e:
        logger.error(f"Error loading import page: {e}", exc_info=True)
        return layout("Import Files",
            H1("📥 Import Files"),
            Article(f"❌ Error: {str(e)}", style="background: var(--pico-del-color); padding: 2rem;")
        )
    finally:
        db.close()


@app.get("/import/browse-pane")
def browse_pane(source_id: int):
    """Load initial browse pane for source root."""
    return browse_directory(source_id, path=".")


@app.get("/import/browse/{source_id}")
def browse_directory(source_id: int, path: str = "."):
    """Browse directory within source root, showing only NEW files."""
    db = next(get_db())
    try:
        from pathlib import Path

        # 1. Get source root
        source_root = db.get(SourceRoot, source_id)
        if not source_root:
            return Div("❌ Source root not found", style="color: var(--pico-del-color);")

        # 2. Resolve paths with security checks
        root_path = Path(source_root.path).resolve()
        target_path = (root_path / path).resolve()

        # Security: Ensure within source root
        if not str(target_path).startswith(str(root_path)):
            return Div("❌ Invalid path", style="color: var(--pico-del-color);")

        if not target_path.exists() or not target_path.is_dir():
            return Div("❌ Directory not found", style="color: var(--pico-del-color);")

        # 3. Get existing inventory
        existing_files = db.query(FileLocation.rel_path).filter(
            FileLocation.source_root_id == source_id
        ).all()
        existing_set = {row.rel_path for row in existing_files}

        # 4. List directory
        entries = []
        try:
            for entry in target_path.iterdir():
                if entry.name.startswith("."):
                    continue  # Skip hidden files

                rel_to_root = str(entry.relative_to(root_path))

                if entry.is_dir():
                    entries.append({
                        "type": "dir",
                        "name": entry.name,
                        "rel_path": rel_to_root
                    })
                elif entry.is_file():
                    entries.append({
                        "type": "file",
                        "name": entry.name,
                        "rel_path": rel_to_root,
                        "size": entry.stat().st_size,
                        "in_inventory": rel_to_root in existing_set
                    })
        except PermissionError:
            return Div("❌ Permission denied", style="color: var(--pico-del-color);")

        # Sort: directories first, then files
        entries.sort(key=lambda x: (x["type"] != "dir", x["name"].lower()))

        # 5. Get plugins for selection
        plugins = db.query(PluginConfig).all()

        # 6. Calculate parent path
        parent_path = str(target_path.parent.relative_to(root_path)) if target_path != root_path else None

        # 7. Build UI
        file_rows = []
        for i, entry in enumerate(entries):
            if entry["type"] == "dir":
                file_rows.append(
                    Div(
                        Span("📁 ", style="font-size: 1.2em;"),
                        A(
                            entry["name"],
                            hx_get=f"/import/browse/{source_id}?path={entry['rel_path']}",
                            hx_target="#browse-pane",
                            hx_swap="innerHTML"
                        ),
                        style="padding: 0.5rem; border-bottom: 1px solid var(--pico-muted-border-color);"
                    )
                )
            else:
                disabled = entry.get("in_inventory", False)
                file_rows.append(
                    Div(
                        Label(
                            Input(
                                type="checkbox",
                                name="files",
                                value=entry["rel_path"],
                                disabled=disabled,
                                style="margin-right: 0.5rem;"
                            ),
                            Span("📄 ", style="font-size: 1.2em;"),
                            entry["name"],
                            Span(
                                f" ({entry['size']:,} bytes)",
                                style="color: var(--pico-muted-color); font-size: 0.9em;"
                            ),
                            Span(
                                " [In Inventory]",
                                style="color: var(--pico-muted-color); font-style: italic;"
                            ) if disabled else "",
                            style="opacity: 0.5;" if disabled else ""
                        ),
                        style="padding: 0.5rem; border-bottom: 1px solid var(--pico-muted-border-color);"
                    )
                )

        return Div(
            Article(
                Header(f"📂 Current: {path if path != '.' else '(root)'}"),
                A(
                    "⬆️ Parent Directory",
                    hx_get=f"/import/browse/{source_id}?path={parent_path if parent_path else '.'}",
                    hx_target="#browse-pane",
                    hx_swap="innerHTML",
                    role="button",
                    cls="secondary"
                ) if parent_path is not None and target_path != root_path else "",
            ),

            Form(
                Article(
                    Header("Files and Directories"),
                    Div(*file_rows) if file_rows else P("No files or directories found."),
                ),

                Article(
                    Header("Import Configuration"),
                    Fieldset(
                        Label(
                            "Tags (comma-separated)",
                            Input(
                                type="text",
                                name="tags",
                                placeholder="e.g., data,raw,experiment1"
                            )
                        ),
                        Legend("Select Plugins (for manual job creation)"),
                        *[
                            Label(
                                Input(
                                    type="checkbox",
                                    name="plugins",
                                    value=p.plugin_name,
                                    style="margin-right: 0.5rem;"
                                ),
                                p.plugin_name
                            ) for p in plugins
                        ] if plugins else [P("No plugins configured")],
                    ),
                ),

                Input(type="hidden", name="source_id", value=source_id),
                Button("📥 Import Selected Files", type="submit"),

                hx_post="/import/submit",
                hx_target="#import-result",
                hx_swap="innerHTML"
            ),

            Div(id="import-result"),
            id="browse-pane"
        )

    except Exception as e:
        logger.error(f"Error browsing directory: {e}", exc_info=True)
        return Div(f"❌ Error: {str(e)}", style="color: var(--pico-del-color);")
    finally:
        db.close()


@app.post("/import/submit")
def import_submit(source_id: int, files: list = None, tags: str = "", plugins: list = None):
    """Handle file import submission."""
    db = next(get_db())
    try:
        from casparian_flow.services.import_service import ImportService

        if not files:
            return Div(
                "⚠️ No files selected",
                cls="toast",
                style="background: var(--pico-mark-color);"
            )

        # Parse tags
        manual_tags = {t.strip() for t in tags.split(",") if t.strip()}

        # Parse plugins
        manual_plugins = set(plugins) if plugins else set()

        # Import files
        import_service = ImportService(db)
        imported = import_service.import_files(
            source_root_id=source_id,
            rel_paths=files if isinstance(files, list) else [files],
            manual_tags=manual_tags,
            manual_plugins=manual_plugins
        )

        if not imported:
            return Div(
                "❌ Failed to import files",
                cls="toast",
                style="background: var(--pico-del-color);"
            )

        return Div(
            H3(f"✅ Successfully imported {len(imported)} file(s)"),
            Ul(*[
                Li(f"{f.filename} (ID: {f.id})") for f in imported
            ]),
            A("View in Inventory →", href="/inventory", role="button"),
            cls="toast"
        )

    except Exception as e:
        logger.error(f"Error importing files: {e}", exc_info=True)
        return Div(
            f"❌ Import failed: {str(e)}",
            cls="toast",
            style="background: var(--pico-del-color);"
        )
    finally:
        db.close()


# For running with uvicorn
serve = app
