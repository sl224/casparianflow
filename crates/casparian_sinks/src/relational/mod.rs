use anyhow::Result;
use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;

pub mod duckdb;

pub enum RelationalBackend {
    DuckDb(duckdb::DuckDbSink),
}

pub struct RelationalSink {
    backend: RelationalBackend,
}

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
