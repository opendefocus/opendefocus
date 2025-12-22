// Precommit hooks

use anyhow::Result;
use duct::cmd;

pub fn precommit() -> Result<()> {
    run_ty_typechecking()?;
    Ok(())
}

fn run_ty_typechecking() -> Result<()> {
    cmd!("uvx", "ty", "check").run()?;
    Ok(())
}