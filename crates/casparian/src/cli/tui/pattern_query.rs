use casparian::scout::SourceId;
use casparian_db::{DbConnection, DbValue};
use globset::GlobMatcher;

#[derive(Debug, Clone, Default)]
pub struct PatternQuery {
    pub extension: Option<String>,
    pub path_pattern: Option<String>,
}

impl PatternQuery {
    pub fn from_glob(pattern: &str) -> Self {
        // Extract extension from end if pattern ends with *.ext
        let extension = if pattern.contains("*.") {
            pattern
                .rsplit("*.")
                .next()
                .filter(|ext| !ext.contains('/') && !ext.contains('*'))
                .map(|ext| ext.to_lowercase())
        } else {
            None
        };

        // Convert remaining pattern to LIKE syntax
        let path_pattern = if pattern.contains('/') || pattern.contains("**") {
            let mut like = pattern
                .replace("**/", "%")
                .replace("**", "%")
                .replace('*', "%")
                .replace('?', "_");

            // Remove extension part if we extracted it (e.g., "%.rs" -> "%")
            if extension.is_some() {
                if let Some(idx) = like.rfind("%.") {
                    like = like[..idx].to_string();
                    if like.is_empty() || like == "%" {
                        return Self {
                            extension,
                            path_pattern: None,
                        };
                    }
                    if !like.ends_with('%') {
                        like.push('%');
                    }
                }
            }

            if like == "%" || like == "%%" || like.is_empty() {
                None
            } else {
                Some(like)
            }
        } else {
            None
        };

        Self {
            extension,
            path_pattern,
        }
    }

    pub fn count_files(&self, conn: &DbConnection, source_id: SourceId) -> i64 {
        let (sql, params) = match (self.extension.as_deref(), self.path_pattern.as_deref()) {
            (Some(ext), Some(path_pat)) => (
                "SELECT COUNT(*) FROM scout_files WHERE source_id = ? AND extension = ? AND rel_path LIKE ?",
                vec![
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(ext.to_string()),
                    DbValue::Text(path_pat.to_string()),
                ],
            ),
            (Some(ext), None) => (
                "SELECT COUNT(*) FROM scout_files WHERE source_id = ? AND extension = ?",
                vec![
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(ext.to_string()),
                ],
            ),
            (None, Some(path_pat)) => (
                "SELECT COUNT(*) FROM scout_files WHERE source_id = ? AND rel_path LIKE ?",
                vec![
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(path_pat.to_string()),
                ],
            ),
            (None, None) => (
                "SELECT COUNT(*) FROM scout_files WHERE source_id = ?",
                vec![DbValue::Integer(source_id.as_i64())],
            ),
        };

        conn.query_scalar::<i64>(sql, &params).unwrap_or(0)
    }

    pub fn search_files(
        &self,
        conn: &DbConnection,
        source_id: SourceId,
        limit: usize,
        offset: usize,
    ) -> Vec<(String, i64, i64)> {
        let (sql, params) = match (self.extension.as_deref(), self.path_pattern.as_deref()) {
            (Some(ext), Some(path_pat)) => (
                r#"SELECT rel_path, size, mtime FROM scout_files
                   WHERE source_id = ? AND extension = ? AND rel_path LIKE ?
                   ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                vec![
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(ext.to_string()),
                    DbValue::Text(path_pat.to_string()),
                    DbValue::Integer(limit as i64),
                    DbValue::Integer(offset as i64),
                ],
            ),
            (Some(ext), None) => (
                r#"SELECT rel_path, size, mtime FROM scout_files
                   WHERE source_id = ? AND extension = ?
                   ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                vec![
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(ext.to_string()),
                    DbValue::Integer(limit as i64),
                    DbValue::Integer(offset as i64),
                ],
            ),
            (None, Some(path_pat)) => (
                r#"SELECT rel_path, size, mtime FROM scout_files
                   WHERE source_id = ? AND rel_path LIKE ?
                   ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                vec![
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Text(path_pat.to_string()),
                    DbValue::Integer(limit as i64),
                    DbValue::Integer(offset as i64),
                ],
            ),
            (None, None) => (
                r#"SELECT rel_path, size, mtime FROM scout_files
                   WHERE source_id = ?
                   ORDER BY mtime DESC LIMIT ? OFFSET ?"#,
                vec![
                    DbValue::Integer(source_id.as_i64()),
                    DbValue::Integer(limit as i64),
                    DbValue::Integer(offset as i64),
                ],
            ),
        };

        let rows = match conn.query_all(sql, &params) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let rel_path: String = match row.get(0) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let size: i64 = match row.get(1) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let mtime: i64 = match row.get(2) {
                Ok(v) => v,
                Err(_) => continue,
            };
            results.push((rel_path, size, mtime));
        }

        results
    }
}

pub fn eval_glob_pattern(pattern: &str) -> Result<String, String> {
    let mut glob_pattern = if pattern.is_empty() {
        "**/*".to_string()
    } else if pattern.contains('<') && pattern.contains('>') {
        super::extraction::parse_custom_glob(pattern)
            .map(|p| p.glob_pattern)
            .map_err(|e| e.message)?
    } else {
        pattern.to_string()
    };

    if glob_pattern == "*" {
        glob_pattern = "**/*".to_string();
    }

    if !glob_pattern.contains('/') && !glob_pattern.starts_with("**/") && glob_pattern != "**/*" {
        glob_pattern = format!("**/{}", glob_pattern);
    }

    Ok(glob_pattern)
}

pub fn build_eval_matcher(glob_pattern: &str) -> Result<GlobMatcher, String> {
    globset::GlobBuilder::new(glob_pattern)
        .case_insensitive(true)
        .build()
        .map(|g| g.compile_matcher())
        .map_err(|_| "Invalid pattern".to_string())
}

pub fn sample_paths_for_eval(
    conn: &DbConnection,
    source_id: SourceId,
    glob_pattern: &str,
    matcher: &GlobMatcher,
) -> Vec<String> {
    let query = PatternQuery::from_glob(glob_pattern);

    let mut where_sql = String::from("source_id = ?");
    let mut base_params: Vec<DbValue> = vec![DbValue::Integer(source_id.as_i64())];

    if let Some(ext) = query.extension.as_deref() {
        where_sql.push_str(" AND extension = ?");
        base_params.push(DbValue::Text(ext.to_string()));
    }
    if let Some(path_pat) = query.path_pattern.as_deref() {
        where_sql.push_str(" AND rel_path LIKE ?");
        base_params.push(DbValue::Text(path_pat.to_string()));
    }

    let prefix_query = format!(
        "SELECT CASE WHEN INSTR(rel_path, '/') > 0 THEN SUBSTR(rel_path, 1, INSTR(rel_path, '/') - 1) ELSE rel_path END AS prefix, COUNT(*) as cnt \
         FROM scout_files WHERE {} GROUP BY prefix ORDER BY cnt DESC LIMIT 50",
        where_sql
    );

    let prefix_rows = match conn.query_all(&prefix_query, &base_params) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    let mut samples: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for row in prefix_rows {
        let prefix: String = match row.get(0) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if prefix.is_empty() {
            continue;
        }

        let mut params = base_params.clone();
        let prefix_like = format!("{}/%", prefix);
        params.push(DbValue::Text(prefix.clone()));
        params.push(DbValue::Text(prefix_like));

        let paths_query = format!(
            "SELECT rel_path FROM scout_files WHERE {} AND (rel_path = ? OR rel_path LIKE ?) LIMIT 50",
            where_sql
        );

        let rows = match conn.query_all(&paths_query, &params) {
            Ok(rows) => rows,
            Err(_) => continue,
        };

        for row in rows {
            let rel_path: String = match row.get(0) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if !matcher.is_match(&rel_path) {
                continue;
            }
            if seen.insert(rel_path.clone()) {
                samples.push(rel_path);
                if samples.len() >= 200 {
                    return samples;
                }
            }
        }
    }

    samples
}
