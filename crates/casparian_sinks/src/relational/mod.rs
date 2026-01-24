#[cfg(feature = "sink-duckdb")]
use anyhow::Result;
#[cfg(feature = "sink-duckdb")]
use arrow::datatypes::Schema;
#[cfg(feature = "sink-duckdb")]
use arrow::record_batch::RecordBatch;

#[cfg(feature = "sink-duckdb")]
pub mod duckdb;

#[cfg(feature = "sink-duckdb")]
pub enum RelationalBackend {
    DuckDb(duckdb::DuckDbSink),
}

#[cfg(feature = "sink-duckdb")]
pub struct RelationalSink {
    backend: RelationalBackend,
}

#[cfg(feature = "sink-duckdb")]
impl RelationalSink {
    pub fn new(backend: RelationalBackend) -> Self {
        Self { backend }
    }

    pub fn init(&mut self, schema: &Schema) -> Result<()> {
        match &mut self.backend {
            RelationalBackend::DuckDb(backend) => backend.init(schema),
        }
    }

    pub fn write_batch(&mut self, batch: &RecordBatch) -> Result<u64> {
        match &mut self.backend {
            RelationalBackend::DuckDb(backend) => backend.write_batch(batch),
        }
    }

    pub fn prepare(&mut self) -> Result<()> {
        match &mut self.backend {
            RelationalBackend::DuckDb(backend) => backend.prepare(),
        }
    }

    pub fn commit(&mut self) -> Result<()> {
        match &mut self.backend {
            RelationalBackend::DuckDb(backend) => backend.commit(),
        }
    }

    pub fn rollback(&mut self) -> Result<()> {
        match &mut self.backend {
            RelationalBackend::DuckDb(backend) => backend.rollback(),
        }
    }
}
