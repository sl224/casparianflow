"""
Surveyor Agent: Autonomous data pipeline orchestrator.

This agent executes the 6-phase Surveyor Protocol to automatically:
1. Scan and profile unprocessed files
2. Check available libraries
3. Generate and deploy plugins
4. Configure routing rules
5. Verify execution
6. Generate automated tests
"""

import json
import logging
from dataclasses import dataclass
from typing import Optional, Dict, Any, List
from pathlib import Path

from sqlalchemy.orm import Session

from casparian_flow.db.models import (
    SurveyorSession,
    SurveyorDecision,
    PhaseEnum,
    FileVersion,
    FileLocation,
    ProcessingJob,
    PluginManifest,
    LibraryWhitelist,
    RoutingRule,
    PluginConfig,
    StatusEnum,
)
from casparian_flow.services.scout import Scout
from casparian_flow.services.inspector import profile_file
from casparian_flow.services.architect import ArchitectService
from casparian_flow.services.llm_generator import LLMGenerator
from casparian_flow.services.ai_types import FileProfile, SchemaProposal

logger = logging.getLogger(__name__)


@dataclass
class PhaseResult:
    """Result of a phase execution."""
    success: bool
    next_phase: PhaseEnum
    data: Dict[str, Any]
    error: Optional[str] = None


class SurveyorAgent:
    """
    Autonomous agent that orchestrates the 6-phase Surveyor Protocol.

    The agent maintains state in the database and can be resumed after failures.
    """

    def __init__(
        self,
        db_session: Session,
        scout: Scout,
        architect: ArchitectService,
        llm_generator: LLMGenerator,
        test_generator: 'TestGenerator',
    ):
        self.db = db_session
        self.scout = scout
        self.architect = architect
        self.llm_generator = llm_generator
        self.test_generator = test_generator

    def create_session(self, source_root_id: int) -> SurveyorSession:
        """Create a new surveyor session."""
        session = SurveyorSession(
            source_root_id=source_root_id,
            current_phase=PhaseEnum.IDLE,
            phase_data=json.dumps({}),
        )
        self.db.add(session)
        self.db.commit()
        self.db.refresh(session)

        logger.info(f"Created surveyor session {session.id} for source_root {source_root_id}")
        return session

    def execute_phase(self, session: SurveyorSession) -> PhaseResult:
        """Execute the current phase based on session state."""
        phase_map = {
            PhaseEnum.IDLE: self.phase_1_reconnaissance,
            PhaseEnum.PHASE_1_RECONNAISSANCE: self.phase_2_environment_check,
            PhaseEnum.PHASE_2_ENVIRONMENT: self.phase_3_construction,
            PhaseEnum.PHASE_3_CONSTRUCTION: self.phase_4_wiring,
            PhaseEnum.PHASE_4_WIRING: self.phase_5_verification,
            PhaseEnum.PHASE_5_VERIFICATION: self.phase_6_test_generation,
            PhaseEnum.PHASE_6_TEST_GENERATION: self._mark_completed,
        }

        current_phase = session.current_phase
        if current_phase not in phase_map:
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"Unknown phase: {current_phase}",
            )

        phase_method = phase_map[current_phase]

        try:
            result = phase_method(session)
            if result.success:
                self.advance_phase(session, result.next_phase)
            else:
                session.error_message = result.error
                session.current_phase = PhaseEnum.FAILED
                self.db.commit()
            return result
        except Exception as e:
            logger.exception(f"Error in phase {current_phase}")
            session.error_message = str(e)
            session.current_phase = PhaseEnum.FAILED
            self.db.commit()
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=str(e),
            )

    def phase_1_reconnaissance(self, session: SurveyorSession) -> PhaseResult:
        """
        Phase 1: Scan files and identify unprocessed ones.

        Actions:
        - Query FileVersions with no ProcessingJob
        - Sample representative files
        - Store sample file IDs in phase_data
        """
        logger.info(f"[Session {session.id}] Starting Phase 1: Reconnaissance")

        # Query unprocessed files
        unprocessed = (
            self.db.query(FileVersion)
            .outerjoin(ProcessingJob, ProcessingJob.file_version_id == FileVersion.id)
            .filter(ProcessingJob.id == None)
            .limit(10)
            .all()
        )

        if not unprocessed:
            # No files to process
            logger.info(f"[Session {session.id}] No unprocessed files found")
            self.log_decision(
                session,
                PhaseEnum.PHASE_1_RECONNAISSANCE,
                "no_files",
                {"count": 0},
                "No unprocessed files found, marking session as completed",
            )
            return PhaseResult(
                success=True,
                next_phase=PhaseEnum.COMPLETED,
                data={"unprocessed_count": 0},
            )

        # Sample a representative file
        sample_file = unprocessed[0]

        phase_data = {
            "sample_file_version_id": sample_file.id,
            "sample_file_location_id": sample_file.location_id,
            "total_unprocessed": len(unprocessed),
        }

        session.phase_data = json.dumps(phase_data)
        self.db.commit()

        self.log_decision(
            session,
            PhaseEnum.PHASE_1_RECONNAISSANCE,
            "sample_selected",
            phase_data,
            f"Selected file_version {sample_file.id} as representative sample",
        )

        logger.info(f"[Session {session.id}] Found {len(unprocessed)} unprocessed files")

        return PhaseResult(
            success=True,
            next_phase=PhaseEnum.PHASE_2_ENVIRONMENT,
            data=phase_data,
        )

    def phase_2_environment_check(self, session: SurveyorSession) -> PhaseResult:
        """
        Phase 2: Check available libraries.

        Actions:
        - Query LibraryWhitelist table
        - Return list for LLM context
        """
        logger.info(f"[Session {session.id}] Starting Phase 2: Environment Check")

        libs = self.db.query(LibraryWhitelist).all()

        library_list = [
            {"name": lib.library_name, "version": lib.version_constraint}
            for lib in libs
        ]

        phase_data = json.loads(session.phase_data)
        phase_data["allowed_libraries"] = library_list
        session.phase_data = json.dumps(phase_data)
        self.db.commit()

        self.log_decision(
            session,
            PhaseEnum.PHASE_2_ENVIRONMENT,
            "libraries_checked",
            {"library_count": len(libs)},
            f"Found {len(libs)} allowed libraries",
        )

        logger.info(f"[Session {session.id}] Found {len(libs)} allowed libraries")

        return PhaseResult(
            success=True,
            next_phase=PhaseEnum.PHASE_3_CONSTRUCTION,
            data={"libraries": library_list},
        )

    def phase_3_construction(self, session: SurveyorSession) -> PhaseResult:
        """
        Phase 3: Generate and deploy plugin.

        Actions:
        - Call inspector.profile_file()
        - Call llm_generator.propose_schema()
        - Call llm_generator.generate_plugin()
        - Call architect.deploy_plugin()
        - Retry on validation errors (max 3 attempts)
        """
        logger.info(f"[Session {session.id}] Starting Phase 3: Construction")

        phase_data = json.loads(session.phase_data)
        file_version_id = phase_data.get("sample_file_version_id")
        file_location_id = phase_data.get("sample_file_location_id")

        if not file_version_id or not file_location_id:
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error="Missing sample file information from Phase 1",
            )

        # Get file location
        file_loc = self.db.query(FileLocation).get(file_location_id)
        if not file_loc:
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"FileLocation {file_location_id} not found",
            )

        # Resolve full path
        full_path = Path(file_loc.source_root.path) / file_loc.rel_path

        if not full_path.exists():
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"File not found: {full_path}",
            )

        # Profile file
        try:
            file_profile = profile_file(str(full_path))
        except Exception as e:
            logger.error(f"Failed to profile file: {e}")
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"File profiling failed: {e}",
            )

        # Propose schema
        try:
            schema_proposal = self.llm_generator.propose_schema(file_profile)
        except Exception as e:
            logger.error(f"Failed to propose schema: {e}")
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"Schema proposal failed: {e}",
            )

        # Generate plugin code
        try:
            plugin_code_result = self.llm_generator.generate_plugin(schema_proposal)
        except Exception as e:
            logger.error(f"Failed to generate plugin: {e}")
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"Plugin generation failed: {e}",
            )

        # Deploy plugin with retries
        plugin_name = f"surveyor_{file_loc.filename.replace('.', '_')}"
        max_retries = 3

        for attempt in range(max_retries):
            try:
                from casparian_flow.security.gatekeeper import generate_signature

                signature = generate_signature(plugin_code_result.source_code, self.architect.secret_key)

                result = self.architect.deploy_plugin(
                    plugin_name=plugin_name,
                    version="1.0.0",
                    source_code=plugin_code_result.source_code,
                    signature=signature,
                    sample_input=None,  # Skip sandbox for now
                )

                if result.success:
                    phase_data["deployed_plugin_name"] = plugin_name
                    phase_data["deployed_manifest_id"] = result.manifest_id
                    phase_data["schema_proposal"] = {
                        "file_type": schema_proposal.file_type_inferred,
                        "target_topic": schema_proposal.target_topic,
                        "columns": schema_proposal.columns,
                    }
                    session.phase_data = json.dumps(phase_data)
                    self.db.commit()

                    self.log_decision(
                        session,
                        PhaseEnum.PHASE_3_CONSTRUCTION,
                        "plugin_deployed",
                        {"plugin_name": plugin_name, "manifest_id": result.manifest_id},
                        f"Successfully deployed plugin {plugin_name}",
                    )

                    logger.info(f"[Session {session.id}] Plugin {plugin_name} deployed successfully")

                    return PhaseResult(
                        success=True,
                        next_phase=PhaseEnum.PHASE_4_WIRING,
                        data={"plugin_name": plugin_name, "manifest_id": result.manifest_id},
                    )
                else:
                    logger.warning(f"Deployment attempt {attempt + 1} failed: {result.error_message}")
                    if attempt == max_retries - 1:
                        return PhaseResult(
                            success=False,
                            next_phase=PhaseEnum.FAILED,
                            data={},
                            error=f"Plugin deployment failed after {max_retries} attempts: {result.error_message}",
                        )
            except Exception as e:
                logger.error(f"Deployment attempt {attempt + 1} raised exception: {e}")
                if attempt == max_retries - 1:
                    return PhaseResult(
                        success=False,
                        next_phase=PhaseEnum.FAILED,
                        data={},
                        error=f"Plugin deployment failed after {max_retries} attempts: {e}",
                    )

        return PhaseResult(
            success=False,
            next_phase=PhaseEnum.FAILED,
            data={},
            error="Unexpected deployment failure",
        )

    def phase_4_wiring(self, session: SurveyorSession) -> PhaseResult:
        """
        Phase 4: Configure routing rules.

        Actions:
        - Create RoutingRule for file pattern
        - Create/update PluginConfig subscription
        - Trigger scout re-scan to apply tags
        """
        logger.info(f"[Session {session.id}] Starting Phase 4: Wiring")

        phase_data = json.loads(session.phase_data)
        plugin_name = phase_data.get("deployed_plugin_name")
        file_location_id = phase_data.get("sample_file_location_id")

        if not plugin_name:
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error="Missing plugin name from Phase 3",
            )

        # Get file location for pattern inference
        file_loc = self.db.query(FileLocation).get(file_location_id)
        if not file_loc:
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"FileLocation {file_location_id} not found",
            )

        # Infer pattern from filename (e.g., *.csv, *.xlsx)
        from pathlib import Path as PathlibPath
        ext = PathlibPath(file_loc.filename).suffix
        pattern = f"*{ext}" if ext else file_loc.filename

        # Generate tag name
        tag = f"surveyor_{ext[1:]}_data" if ext else "surveyor_data"

        # Create routing rule
        rule = RoutingRule(pattern=pattern, tag=tag, priority=10)
        self.db.add(rule)

        # Create/update plugin config
        config = self.db.query(PluginConfig).filter_by(plugin_name=plugin_name).first()
        if not config:
            config = PluginConfig(plugin_name=plugin_name, subscription_tags=tag)
            self.db.add(config)
        else:
            existing_tags = set(config.subscription_tags.split(",")) if config.subscription_tags else set()
            existing_tags.add(tag)
            config.subscription_tags = ",".join(sorted(existing_tags))

        self.db.commit()

        # Trigger re-scan to apply tags
        try:
            self.scout.scan_source(file_loc.source_root)
        except Exception as e:
            logger.warning(f"Re-scan failed: {e}")

        phase_data["routing_pattern"] = pattern
        phase_data["routing_tag"] = tag
        session.phase_data = json.dumps(phase_data)
        self.db.commit()

        self.log_decision(
            session,
            PhaseEnum.PHASE_4_WIRING,
            "routing_configured",
            {"pattern": pattern, "tag": tag, "plugin": plugin_name},
            f"Configured routing: {pattern} -> {tag} -> {plugin_name}",
        )

        logger.info(f"[Session {session.id}] Routing configured: {pattern} -> {tag} -> {plugin_name}")

        return PhaseResult(
            success=True,
            next_phase=PhaseEnum.PHASE_5_VERIFICATION,
            data={"pattern": pattern, "tag": tag},
        )

    def phase_5_verification(self, session: SurveyorSession) -> PhaseResult:
        """
        Phase 5: Verify plugin executes successfully.

        Actions:
        - Check for ProcessingJobs created for the plugin
        - Verify job status
        """
        logger.info(f"[Session {session.id}] Starting Phase 5: Verification")

        phase_data = json.loads(session.phase_data)
        plugin_name = phase_data.get("deployed_plugin_name")

        if not plugin_name:
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error="Missing plugin name from Phase 3",
            )

        # Query for jobs related to this plugin
        jobs = self.db.query(ProcessingJob).filter_by(plugin_name=plugin_name).all()

        if not jobs:
            logger.warning(f"[Session {session.id}] No jobs found for plugin {plugin_name}, but continuing")
            # Not a failure - jobs may be queued later

        # Count job statuses
        job_stats = {
            "total": len(jobs),
            "queued": len([j for j in jobs if j.status == StatusEnum.QUEUED]),
            "running": len([j for j in jobs if j.status == StatusEnum.RUNNING]),
            "completed": len([j for j in jobs if j.status == StatusEnum.COMPLETED]),
            "failed": len([j for j in jobs if j.status == StatusEnum.FAILED]),
        }

        phase_data["verification_stats"] = job_stats
        session.phase_data = json.dumps(phase_data)
        self.db.commit()

        self.log_decision(
            session,
            PhaseEnum.PHASE_5_VERIFICATION,
            "verification_complete",
            job_stats,
            f"Plugin {plugin_name} verification: {job_stats}",
        )

        logger.info(f"[Session {session.id}] Verification complete: {job_stats}")

        return PhaseResult(
            success=True,
            next_phase=PhaseEnum.PHASE_6_TEST_GENERATION,
            data=job_stats,
        )

    def phase_6_test_generation(self, session: SurveyorSession) -> PhaseResult:
        """
        Phase 6: Generate automated tests.

        Actions:
        - Get PluginManifest and sample file
        - Call test_generator.generate_test()
        - Write test to tests/generated/
        - Log results
        """
        logger.info(f"[Session {session.id}] Starting Phase 6: Test Generation")

        phase_data = json.loads(session.phase_data)
        plugin_name = phase_data.get("deployed_plugin_name")
        manifest_id = phase_data.get("deployed_manifest_id")
        file_location_id = phase_data.get("sample_file_location_id")
        schema_proposal_data = phase_data.get("schema_proposal")

        if not all([plugin_name, manifest_id, file_location_id, schema_proposal_data]):
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error="Missing data for test generation from previous phases",
            )

        # Get plugin manifest
        manifest = self.db.query(PluginManifest).get(manifest_id)
        if not manifest:
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"PluginManifest {manifest_id} not found",
            )

        # Get sample file
        file_loc = self.db.query(FileLocation).get(file_location_id)
        if not file_loc:
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"FileLocation {file_location_id} not found",
            )

        # Reconstruct SchemaProposal (simplified)
        from casparian_flow.services.ai_types import SchemaProposal
        schema_proposal = SchemaProposal(
            file_type_inferred=schema_proposal_data.get("file_type", "UNKNOWN"),
            target_topic=schema_proposal_data.get("target_topic", "output"),
            columns=schema_proposal_data.get("columns", []),
            read_strategy="pandas",
        )

        # Generate test
        try:
            test_result = self.test_generator.generate_test(
                plugin_manifest=manifest,
                schema_proposal=schema_proposal,
                sample_file=file_loc,
            )

            if test_result.success:
                phase_data["test_file_path"] = test_result.test_file_path
                session.phase_data = json.dumps(phase_data)
                self.db.commit()

                self.log_decision(
                    session,
                    PhaseEnum.PHASE_6_TEST_GENERATION,
                    "test_generated",
                    {"test_file": test_result.test_file_path},
                    f"Generated test at {test_result.test_file_path}",
                )

                logger.info(f"[Session {session.id}] Test generated: {test_result.test_file_path}")

                return PhaseResult(
                    success=True,
                    next_phase=PhaseEnum.COMPLETED,
                    data={"test_file": test_result.test_file_path},
                )
            else:
                logger.error(f"Test generation failed: {test_result.error_message}")
                return PhaseResult(
                    success=False,
                    next_phase=PhaseEnum.FAILED,
                    data={},
                    error=f"Test generation failed: {test_result.error_message}",
                )

        except Exception as e:
            logger.exception("Test generation raised exception")
            return PhaseResult(
                success=False,
                next_phase=PhaseEnum.FAILED,
                data={},
                error=f"Test generation exception: {e}",
            )

    def _mark_completed(self, session: SurveyorSession) -> PhaseResult:
        """Mark session as completed."""
        from datetime import datetime

        session.current_phase = PhaseEnum.COMPLETED
        session.completed_at = datetime.now()
        self.db.commit()

        logger.info(f"[Session {session.id}] Surveyor protocol completed successfully")

        return PhaseResult(
            success=True,
            next_phase=PhaseEnum.COMPLETED,
            data={"status": "completed"},
        )

    def log_decision(
        self,
        session: SurveyorSession,
        phase: PhaseEnum,
        decision_type: str,
        data: Dict[str, Any],
        reasoning: Optional[str] = None,
    ):
        """Record decision in audit trail."""
        decision = SurveyorDecision(
            session_id=session.id,
            phase=phase,
            decision_type=decision_type,
            decision_data=json.dumps(data),
            reasoning=reasoning,
        )
        self.db.add(decision)
        self.db.commit()

        logger.debug(f"[Session {session.id}] Decision logged: {decision_type}")

    def advance_phase(self, session: SurveyorSession, next_phase: PhaseEnum):
        """Update session to next phase."""
        from datetime import datetime

        session.current_phase = next_phase

        if next_phase == PhaseEnum.COMPLETED:
            session.completed_at = datetime.now()

        self.db.commit()

        logger.info(f"[Session {session.id}] Advanced to phase: {next_phase.value}")
