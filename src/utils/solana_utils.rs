use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SolanaKeypair(pub Vec<u8>);

impl SolanaKeypair {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        let keypair: Vec<u8> = serde_json::from_str(&data)?;
        Ok(SolanaKeypair(keypair))
    }
    
    pub fn public_key(&self) -> String {
        // En una implementación real, derivaríamos la clave pública
        // Por ahora, retornamos una clave dummy
        "DUMMY_PUBLIC_KEY".to_string()
    }
}