use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::error::Result;
use anyhow::anyhow;
use log::error;
use tokio::{fs::File, process::Command};

//utility function to load a kernel module
pub async fn load_kmodule(
    module_name: &str, args: Option<&[&str]>,
) -> Result<()> {
    let mut cmd = Command::new("modprobe"); //.arg(module_name).status().await?
                                            //add argument if any
    let cmd = match args {
        Some(args) => cmd.arg(module_name).args(args),
        None => cmd.arg(module_name),
    };

    let status = cmd.status().await?;

    if status.success() {
        Ok(())
    } else {
        error!(
            "Failed to load module: {}, please install the module",
            module_name
        );
        Err(anyhow!("Failed to load module"))
    }
}

pub async fn update_dir_permissions<P>(dir: P, mode: &str) -> Result<()>
where
    P: AsRef<Path>,
{
    let status =
        Command::new("chmod").arg(mode).arg(dir.as_ref()).status().await?;

    if status.success() {
        Ok(())
    } else {
        error!("Failed to update directory permissions");
        Err(anyhow!("Failed to update directory permissions"))
    }
}

//utility function to unload a kernel module
//turn into aync when async_drop is available
pub fn unload_kmodule(module_name: &str) -> Result<()> {
    //use std::process::Command to unload the module synchronously
    let status = std::process::Command::new("modprobe")
        .arg("-r")
        .arg(module_name)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        error!("Failed to unload module: {}", module_name);
        Err(anyhow!("Failed to unload module"))
    }
}

//utility function to check if a kernel module is loaded
pub async fn is_kmodule_loaded<P>(
    reg_module_file: P, module_name: &str,
) -> Result<bool>
where
    P: AsRef<Path>,
{
    let file = File::open(&reg_module_file).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.starts_with(module_name) {
            return Ok(true);
        }
    }

    Ok(false)
}
