use anyhow::Result;

pub(crate) trait ConfigEntity {
    fn finalize(&mut self) -> Result<()>;
    fn validate(&self) -> Result<()>;
}
