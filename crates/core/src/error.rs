//! Erro único do núcleo. Já carrega o status HTTP-like que a interface espera
//! (400 = pedido inválido, 404 = não existe, 409 = conflito...), de modo que a
//! camada de API só repassa.

use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone)]
pub struct Error {
    pub status: u16,
    pub message: String,
    /// Dados extras para a interface (ex.: licenças a aceitar, AVDs em uso).
    pub data: Option<Value>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Error { status, message: message.into(), data: None }
    }
    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(400, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(404, message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(409, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(500, message)
    }
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::internal(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::bad(e.to_string())
    }
}

/// Cria um `Error` 400 com mensagem bilíngue: `bad!("pt {}", "en {}", args...)`.
#[macro_export]
macro_rules! bad {
    ($pt:literal, $en:literal $(, $a:expr)* $(,)?) => {
        $crate::error::Error::bad($crate::trf!($pt, $en $(, $a)*))
    };
}

/// Idem, com status à escolha: `err!(404, "pt", "en", args...)`.
#[macro_export]
macro_rules! err {
    ($st:expr, $pt:literal, $en:literal $(, $a:expr)* $(,)?) => {
        $crate::error::Error::new($st, $crate::trf!($pt, $en $(, $a)*))
    };
}
