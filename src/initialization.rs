use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::error;

const INIT_FILE_TEMPLATE: &str = r#"% This file is generated by vesti
docclass article

startdoc

Hello, World!
"#;

pub fn generate_vesti_file(mut project_name: PathBuf) -> error::Result<()> {
    project_name.set_extension("ves");

    let mut file = fs::File::create(project_name)?;
    write!(file, "{INIT_FILE_TEMPLATE}")?;

    Ok(())
}
