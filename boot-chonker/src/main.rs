use std::{fs::File, io::{Read, Write}, path::PathBuf};
use poly1305::{Block, Key, Poly1305, universal_hash::{NewUniversalHash, UniversalHash}};
use structopt::StructOpt;
use anachro_boot::consts;
use uuid::Uuid;

use boot_chonker::{Chunk, TomlChunk, TomlOut};

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    #[structopt(parse(from_os_str))]
    output: PathBuf,

    #[structopt(parse(from_os_str))]
    flashable: Option<PathBuf>,
}

fn main() -> Result<(), ()> {
    let opt = Opt::from_args();

    let mut file = File::open(opt.input).map_err(drop)?;
    // TODO: probably could "with capacity..."
    let mut buf = vec![];
    let read_bytes = file.read_to_end(&mut buf).map_err(drop)?;

    println!("Read: {:?}", read_bytes);

    let key = Key::from_slice(crate::consts::POLY_1305_KEY);
    let mut ttl_poly = Poly1305::new(key);
    let mut chunks = vec![];

    let mut ch_idx = 0;

    for chunk in buf.chunks(consts::CHUNK_SIZE_256B) {
        let mut ch_out = [0xFF; consts::CHUNK_SIZE_256B];
        ch_out[..chunk.len()].copy_from_slice(chunk);

        let key = Key::from_slice(crate::consts::POLY_1305_KEY);
        let mut poly = Poly1305::new(key);

        for chch in ch_out.chunks_exact(consts::POLY_TAG_SIZE) {
            poly.update(Block::from_slice(chch));
            ttl_poly.update(Block::from_slice(chch));
        }

        let result = poly.finalize().into_bytes().into();

        chunks.push(Chunk {
            data: ch_out,
            signature: result,
            chunk: ch_idx,
        });
        ch_idx += 1;
    }

    let ch_per_page = consts::PAGE_SIZE_4K / consts::CHUNK_SIZE_256B;
    let remainder = chunks.len() % ch_per_page;
    let to_fill = ch_per_page - remainder;

    for _ in 0..to_fill {
        let ch_out = [0xFF; consts::CHUNK_SIZE_256B];

        let key = Key::from_slice(crate::consts::POLY_1305_KEY);
        let mut poly = Poly1305::new(key);

        for chch in ch_out.chunks_exact(consts::POLY_TAG_SIZE) {
            poly.update(Block::from_slice(chch));
            ttl_poly.update(Block::from_slice(chch));
        }

        let result = poly.finalize().into_bytes().into();

        chunks.push(Chunk {
            data: ch_out,
            signature: result,
            chunk: ch_idx,
        });
        ch_idx += 1;
    }

    let ttl_result: [u8; 16] = ttl_poly.finalize().into_bytes().into();
    println!("{:02X?}", ttl_result);

    let mut tchunks = vec![];

    for (page_idx, ch) in chunks.iter().enumerate() {
            let out = TomlChunk::from_chunk(ch, page_idx);
            tchunks.push(out);
    }

    println!("256 Byte Chunks: {}", chunks.len());
    println!("4K Sectors     : {}", chunks.len() / ch_per_page);

    let out = TomlOut {
        name: "test".into(),
        uuid: Uuid::new_v4(),
        full_signature: base64::encode(&ttl_result),
        full_size: chunks.len() * consts::CHUNK_SIZE_256B,
        chunks: tchunks,
    };

    let mut outfile = File::create(opt.output).map_err(drop)?;
    let contents = toml::to_vec(&out).map_err(drop)?;
    outfile.write_all(&contents).map_err(drop)?;

    if let Some(ofile) = opt.flashable {
        write_flashable(
            ofile,
            &out.uuid,
            &ttl_result,
            chunks.len(),
            &chunks
        ).map_err(drop)?;
    }

    Ok(())
}


fn write_flashable(
    outfile: PathBuf,
    uuid: &Uuid,
    ttl_poly: &[u8; 16],
    ttl_chunks: usize,
    chunks: &[Chunk],
) -> std::io::Result<()> {
    let mut outfile = File::create(outfile)?;
    let mut outdata = vec![];

    // 0..16
    outdata.extend_from_slice(uuid.as_bytes());

    // 16..32
    outdata.extend_from_slice(ttl_poly);

    // 32..36
    let ttl_chunks = (ttl_chunks as u32).to_le_bytes();
    outdata.extend_from_slice(&ttl_chunks);

    // PAD TO 4096
    while outdata.len() < 4096 {
        outdata.push(0xFF);
    }

    for ch in chunks {
        outdata.extend_from_slice(&ch.data);
    }

    outfile.write_all(&outdata)
}
