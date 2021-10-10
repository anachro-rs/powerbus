use std::{fs::File, io::{Read, Write}, path::PathBuf};
use poly1305::{Block, Key, Poly1305, universal_hash::{NewUniversalHash, UniversalHash}};
use structopt::StructOpt;
use anachro_boot::consts::{self, CHUNK_SIZE};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(StructOpt, Debug)]
struct Opt {
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    #[structopt(parse(from_os_str))]
    output: PathBuf,

    #[structopt(parse(from_os_str))]
    flashable: Option<PathBuf>,
}

struct Chunk {
    data: [u8; consts::CHUNK_SIZE],
    signature: [u8; consts::POLY_TAG_SIZE],
}

#[derive(Serialize, Deserialize, Debug)]
struct FakeDefmtItem {
    idx: usize,
    msg: String,
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

    for chunk in buf.chunks(consts::CHUNK_SIZE) {
        let mut ch_out = [0xFF; consts::CHUNK_SIZE];
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
        });
    }

    let ch_per_page = consts::PAGE_SIZE / consts::CHUNK_SIZE;
    let remainder = chunks.len() % ch_per_page;
    let to_fill = ch_per_page - remainder;

    for _ in 0..to_fill {
        let ch_out = [0xFF; consts::CHUNK_SIZE];

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
        });
    }

    let ttl_result: [u8; 16] = ttl_poly.finalize().into_bytes().into();
    println!("{:02X?}", ttl_result);

    let mut tchunks = vec![];

    for (page_idx, chs) in chunks.chunks(ch_per_page).enumerate() {
        for (ch_idx, ch) in chs.iter().enumerate() {
            let out = TomlChunk::from_chunk(ch, page_idx, ch_idx);
            tchunks.push(out);
        }
    }

    println!("Chunks: {}", chunks.len());
    println!("Pages : {}", chunks.len() / ch_per_page);

    let out = TomlOut {
        name: "test".into(),
        uuid: Uuid::new_v4(),
        full_signature: base64::encode(&ttl_result),
        full_size: chunks.len() * CHUNK_SIZE,
        fake_defmt: fake_defmt(),
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
            chunks.len() / ch_per_page,
            &chunks
        ).map_err(drop)?;
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct TomlOut {
    name: String,
    uuid: Uuid,
    full_size: usize,
    full_signature: String,
    fake_defmt: Vec<FakeDefmtItem>,
    chunks: Vec<TomlChunk>,
}

#[derive(Serialize, Deserialize, Debug)]
struct TomlChunk {
    page: usize,
    chunk: usize,
    data: String,
    signature: String,
}

impl TomlChunk {
    pub fn from_chunk(chunk: &Chunk, page_idx: usize, chunk_idx: usize) -> Self {
        Self {
            data: base64::encode(&chunk.data),
            signature: base64::encode(&chunk.signature),
            page: page_idx,
            chunk: chunk_idx,
        }
    }
}

fn fake_defmt() -> Vec<FakeDefmtItem> {
    let mut ret = vec![];
    ret.push(FakeDefmtItem { idx: 0, msg: r#"{"package":"defmt","tag":"defmt_prim","data":"{=__internal_Display}","disambiguator":"14725451269531928465"}"#.into() });
    ret.push(FakeDefmtItem { idx: 1, msg: r#"{"package":"defmt","tag":"defmt_prim","data":"Unwrap of a None option value","disambiguator":"2008344177882348342"}"#.into() });
    ret.push(FakeDefmtItem { idx: 2, msg: r#"{"package":"anachro-boot","tag":"defmt_info","data":"Hello, world!","disambiguator":"5235508876606808894"}"#.into() });
    ret.push(FakeDefmtItem { idx: 3, msg: r#"{"package":"anachro-boot","tag":"defmt_error","data":"panicked at 'unwrap failed: Peripherals :: take()'\nerror: `{:?}`","disambiguator":"11621442155928179442"}"#.into() });
    ret.push(FakeDefmtItem { idx: 4, msg: r#"{"package":"anachro-boot","tag":"defmt_timestamp","data":"{=u32:010}","disambiguator":"2516557154532536493"}"#.into() });
    ret.push(FakeDefmtItem { idx: 12, msg: r#"{"package":"panic-probe","tag":"defmt_error","data":"{}","disambiguator":"518460688487270049"}"#.into() });

    // What are these?
    ret.push(FakeDefmtItem { idx: 1, msg: r#""_defmt_version_ = 0.2""#.into() });
    ret.push(FakeDefmtItem { idx: 12, msg: r#"__DEFMT_MARKER_TIMESTAMP_WAS_DEFINED"#.into() });


    ret
}

fn write_flashable(
    outfile: PathBuf,
    uuid: &Uuid,
    ttl_poly: &[u8; 16],
    ttl_pages: usize,
    chunks: &[Chunk],
) -> std::io::Result<()> {
    let mut outfile = File::create(outfile)?;
    let mut outdata = vec![];

    // 0..16
    outdata.extend_from_slice(uuid.as_bytes());

    // 16..32
    outdata.extend_from_slice(ttl_poly);

    // 32..36
    let ttl_pages = (ttl_pages as u32).to_le_bytes();
    outdata.extend_from_slice(&ttl_pages);

    // PAD TO 4096
    while outdata.len() < 4096 {
        outdata.push(0xFF);
    }

    for ch in chunks {
        outdata.extend_from_slice(&ch.data);
    }

    outfile.write_all(&outdata)
}
