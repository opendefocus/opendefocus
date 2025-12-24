// Precommit hooks

use anyhow::Result;
use duct::cmd;

use crate::compile::compile_spirv;

pub async fn precommit(args: Vec<String>) -> Result<()> {
    run_ty_typechecking()?;
    if args.iter().any(|arg| arg.contains("kernel")) {
        compile_spirv().await?;
    }
    Ok(())
}

fn run_ty_typechecking() -> Result<()> {
    cmd!("uvx", "ty", "check").run()?;
    Ok(())
}
