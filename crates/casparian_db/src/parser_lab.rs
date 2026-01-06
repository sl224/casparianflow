//! Parser Lab database operations

use crate::error::{DbError, Result};
use crate::types::*;
use crate::CasparianDb;
use sqlx::Row;

impl CasparianDb {
    // ========================================================================
    // Parser Operations
    // ========================================================================

    /// Create a new parser
    pub async fn parser_create(&self, id: &str, name: &str, file_pattern: &str) -> Result<Parser> {
        let now = Self::now_millis();

        sqlx::query(
            r#"
            INSERT INTO parser_lab_parsers (id, name, file_pattern, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(file_pattern)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        self.parser_get(id)
            .await?
            .ok_or_else(|| DbError::not_found("Parser not found after creation"))
    }

    /// Get a parser by ID
    pub async fn parser_get(&self, id: &str) -> Result<Option<Parser>> {
        let row = sqlx::query("SELECT * FROM parser_lab_parsers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_parser(&row)?)),
            None => Ok(None),
        }
    }

    /// List all parsers
    pub async fn parser_list(&self, limit: Option<usize>) -> Result<Vec<Parser>> {
        let sql = match limit {
            Some(n) => format!(
                "SELECT * FROM parser_lab_parsers ORDER BY updated_at DESC LIMIT {}",
                n
            ),
            None => "SELECT * FROM parser_lab_parsers ORDER BY updated_at DESC".to_string(),
        };

        let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
        rows.iter().map(|row| self.row_to_parser(row)).collect()
    }

    /// Update a parser
    pub async fn parser_update(&self, parser: &Parser) -> Result<()> {
        let now = Self::now_millis();

        sqlx::query(
            r#"
            UPDATE parser_lab_parsers SET
                name = ?,
                file_pattern = ?,
                pattern_type = ?,
                source_code = ?,
                source_hash = ?,
                validation_status = ?,
                validation_error = ?,
                validation_output = ?,
                schema_json = ?,
                messages_json = ?,
                sink_type = ?,
                sink_config_json = ?,
                published_at = ?,
                published_plugin_id = ?,
                is_sample = ?,
                output_mode = ?,
                detected_topics_json = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&parser.name)
        .bind(&parser.file_pattern)
        .bind(&parser.pattern_type)
        .bind(&parser.source_code)
        .bind(&parser.source_hash)
        .bind(&parser.validation_status)
        .bind(&parser.validation_error)
        .bind(&parser.validation_output)
        .bind(&parser.schema_json)
        .bind(&parser.messages_json)
        .bind(&parser.sink_type)
        .bind(&parser.sink_config_json)
        .bind(parser.published_at.map(|dt| dt.timestamp_millis()))
        .bind(&parser.published_plugin_id)
        .bind(parser.is_sample)
        .bind(&parser.output_mode)
        .bind(&parser.detected_topics_json)
        .bind(now)
        .bind(&parser.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a parser (cascade deletes test files)
    pub async fn parser_delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM parser_lab_parsers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Update parser validation results
    pub async fn parser_set_validation(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
        output: Option<&str>,
        schema_json: Option<&str>,
        messages_json: Option<&str>,
    ) -> Result<()> {
        let now = Self::now_millis();

        sqlx::query(
            r#"
            UPDATE parser_lab_parsers SET
                validation_status = ?,
                validation_error = ?,
                validation_output = ?,
                schema_json = ?,
                messages_json = ?,
                last_validated_at = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(status)
        .bind(error)
        .bind(output)
        .bind(schema_json)
        .bind(messages_json)
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    fn row_to_parser(&self, row: &sqlx::sqlite::SqliteRow) -> Result<Parser> {
        let created_at: i64 = row.get("created_at");
        let updated_at: i64 = row.get("updated_at");
        let published_at: Option<i64> = row.get("published_at");

        Ok(Parser {
            id: row.get("id"),
            name: row.get("name"),
            file_pattern: row.get("file_pattern"),
            pattern_type: row.get("pattern_type"),
            source_code: row.get("source_code"),
            source_hash: row.get("source_hash"),
            validation_status: row.get("validation_status"),
            validation_error: row.get("validation_error"),
            validation_output: row.get("validation_output"),
            schema_json: row.get("schema_json"),
            messages_json: row.get("messages_json"),
            sink_type: row.get("sink_type"),
            sink_config_json: row.get("sink_config_json"),
            published_at: published_at.map(Self::millis_to_datetime),
            published_plugin_id: row.get("published_plugin_id"),
            is_sample: row.get("is_sample"),
            output_mode: row.get("output_mode"),
            detected_topics_json: row.get("detected_topics_json"),
            created_at: Self::millis_to_datetime(created_at),
            updated_at: Self::millis_to_datetime(updated_at),
        })
    }

    // ========================================================================
    // Test File Operations
    // ========================================================================

    /// Add a test file to a parser
    pub async fn parser_add_test_file(
        &self,
        id: &str,
        parser_id: &str,
        file_path: &str,
        file_name: &str,
        file_size: Option<i64>,
    ) -> Result<ParserTestFile> {
        let now = Self::now_millis();

        sqlx::query(
            r#"
            INSERT INTO parser_lab_test_files (id, parser_id, file_path, file_name, file_size, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(parser_id)
        .bind(file_path)
        .bind(file_name)
        .bind(file_size)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(ParserTestFile {
            id: id.to_string(),
            parser_id: parser_id.to_string(),
            file_path: file_path.to_string(),
            file_name: file_name.to_string(),
            file_size,
            created_at: Self::millis_to_datetime(now),
        })
    }

    /// List test files for a parser
    pub async fn parser_list_test_files(&self, parser_id: &str) -> Result<Vec<ParserTestFile>> {
        let rows = sqlx::query(
            "SELECT * FROM parser_lab_test_files WHERE parser_id = ? ORDER BY created_at DESC",
        )
        .bind(parser_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(|row| self.row_to_test_file(row)).collect()
    }

    /// Remove a test file
    pub async fn parser_remove_test_file(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM parser_lab_test_files WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get a test file by ID
    pub async fn parser_get_test_file(&self, id: &str) -> Result<Option<ParserTestFile>> {
        let row = sqlx::query("SELECT * FROM parser_lab_test_files WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(Some(self.row_to_test_file(&row)?)),
            None => Ok(None),
        }
    }

    fn row_to_test_file(&self, row: &sqlx::sqlite::SqliteRow) -> Result<ParserTestFile> {
        let created_at: i64 = row.get("created_at");

        Ok(ParserTestFile {
            id: row.get("id"),
            parser_id: row.get("parser_id"),
            file_path: row.get("file_path"),
            file_name: row.get("file_name"),
            file_size: row.get("file_size"),
            created_at: Self::millis_to_datetime(created_at),
        })
    }
}
