use anachro_boot::consts;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

pub struct Chunk {
    pub data: [u8; consts::CHUNK_SIZE_256B],
    pub signature: [u8; consts::POLY_TAG_SIZE],
    pub chunk: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TomlOut {
    pub name: String,
    pub uuid: Uuid,
    pub full_size: usize,
    pub full_signature: String,
    pub chunks: Vec<TomlChunk>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TomlChunk {
    pub page: usize,
    pub data: String,
    pub signature: String,
}

impl TomlChunk {
    pub fn from_chunk(chunk: &Chunk, page_idx: usize) -> Self {
        Self {
            data: base64::encode(&chunk.data),
            signature: base64::encode(&chunk.signature),
            page: page_idx,
        }
    }
}

pub fn toml_chunks_to_chunks(tchunks: &Vec<TomlChunk>) -> Vec<Chunk> {
    let mut tchunks = tchunks.clone();

    // Make sure all the chunks are in-order
    tchunks.sort_unstable_by_key(|k| {
        k.page
    });

    tchunks
        .into_iter()
        .map(|tc| {
            let ddata = base64::decode(&tc.data).unwrap();
            let dsign = base64::decode(&tc.signature).unwrap();

            let mut chunk = Chunk {
                data: [0u8; consts::CHUNK_SIZE_256B],
                signature: [0u8; consts::POLY_TAG_SIZE],
                chunk: tc.page,
            };

            chunk.data.copy_from_slice(&ddata);
            chunk.signature.copy_from_slice(&dsign);

            chunk
        })
        .collect()
}
