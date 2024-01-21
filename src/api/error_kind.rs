use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorKind {
    EnvVarError,
    NetworkError,
    ParsingError,
    CouldNotCreateFolder(String),
    CouldNotCreateFile(String),
    CouldNotOpenFile(String),

    CouldNotWriteToCsv(String),
}
