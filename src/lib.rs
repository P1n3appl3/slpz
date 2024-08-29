//! Compresses and decompresses between the slp and slpz formats.
//!
//! You can expect slpz files to be around 8x to 12x times smaller than slp files for regular matches.
//! (~3Mb down to ~300Kb).
//!
//! Compression is done with the zstd compression library. 
//! zstd is not required on the user's computer; the library is statically linked at compile time.
//!
//! The slpz format is documented in the readme in the repo.
//! Important information, such as player tags, stages, date, characters, etc. all remain uncompressed in the slpz format. 
//! This allows slp file browsers to easily parse and display this information without
//! needing to pull in zstd.

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum CompError {
    InvalidFile,
    CompressionFailure,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DecompError {
    InvalidFile,
    VersionTooNew,
    DecompressionFailure,
}

pub type CompResult<T> = Result<T, CompError>;
pub type DecompResult<T> = Result<T, DecompError>;

const EVENT_PAYLOADS: u8 = 0x35;
const GAME_START: u8 = 0x36;
const RAW_HEADER: [u8; 11] = [0x7B, 0x55, 0x03, 0x72, 0x61, 0x77, 0x5B, 0x24, 0x55, 0x23, 0x6C];

pub const VERSION: u32 = 0;

pub struct Compressor { ctx: zstd::bulk::Compressor<'static> }
pub struct Decompressor { ctx: zstd::bulk::Decompressor<'static> }

impl Compressor {
    /// compression_level should be between 0..19. The default is 3.
    pub fn new(compression_level: i32) -> Option<Compressor> {
        Some(Compressor {
            ctx: zstd::bulk::Compressor::new(compression_level).ok()?
        })
    }
}

impl Decompressor {
    pub fn new() -> Option<Decompressor> {
        Some(Decompressor { ctx: zstd::bulk::Decompressor::new().ok()? })
    }
}

/// Compresses an slp file to an slpz file.
pub fn compress(compressor: &mut Compressor, slp: &[u8]) -> CompResult<Vec<u8>> {
    if slp.len() < 16 { return Err(CompError::InvalidFile) }
    if &slp[0..11] != &RAW_HEADER { return Err(CompError::InvalidFile) }

    // get metadata
    let raw_len = u32::from_be_bytes(slp[11..15].try_into().unwrap()) as usize;
    let metadata_offset = 15+raw_len;
    let metadata = &slp[metadata_offset..];

    // get event sizes
    if slp[15] != EVENT_PAYLOADS { return Err(CompError::InvalidFile) }
    let (event_sizes, event_type_count) = event_sizes(&slp[15..]).ok_or(CompError::InvalidFile)?;
    let event_sizes_size = 2+event_type_count*3;
    let event_sizes_payload = &slp[15..][..event_sizes_size];

    // get game start
    let game_start_offset = 15 + event_sizes_size;
    let game_start_size = event_sizes[GAME_START as usize] as usize + 1;
    if slp.len() < game_start_offset+game_start_size { return Err(CompError::InvalidFile) }
    if slp[game_start_offset] != GAME_START { return Err(CompError::InvalidFile) }
    let game_start_payload = &slp[game_start_offset..][..game_start_size];

    let mut slpz = Vec::with_capacity(slp.len());

    // header
    slpz.extend_from_slice(&VERSION.to_be_bytes());
    slpz.extend_from_slice(&[0u8; 20]); // offsets filled later

    // write event sizes
    let len = slpz.len() as u32;
    slpz[4..8].copy_from_slice(&len.to_be_bytes());
    slpz.extend_from_slice(event_sizes_payload);

    // write game start
    let len = slpz.len() as u32;
    slpz[8..12].copy_from_slice(&len.to_be_bytes());
    slpz.extend_from_slice(game_start_payload);

    // write metadata
    let len = slpz.len() as u32;
    slpz[12..16].copy_from_slice(&len.to_be_bytes());
    slpz.extend_from_slice(metadata);

    // write compressed events
    let len = slpz.len() as u32;
    slpz[16..20].copy_from_slice(&len.to_be_bytes());

    let other_events_offset = game_start_offset+game_start_size;
    let mut reordered_data = Vec::with_capacity(slp.len());
    let written = reorder_events(&slp[other_events_offset..metadata_offset], &event_sizes, &mut reordered_data)?;
    slpz[20..24].copy_from_slice(&(written as u32).to_be_bytes());

    // wrap in cursor so we don't overwrite previous data
    let mut slpz_cursor = std::io::Cursor::new(slpz);
    slpz_cursor.set_position(len as u64);
    compressor.ctx.compress_to_buffer(&reordered_data, &mut slpz_cursor).map_err(|_| CompError::CompressionFailure)?;

    Ok(slpz_cursor.into_inner())
}

/// Decompresses an slpz file to an slp file.
pub fn decompress(decompressor: &mut Decompressor, slpz: &[u8]) -> DecompResult<Vec<u8>> {
    if slpz.len() < 24 { return Err(DecompError::InvalidFile) }
    let version                  = u32::from_be_bytes(slpz[0..4].try_into().unwrap());
    let event_sizes_offset       = u32::from_be_bytes(slpz[4..8].try_into().unwrap()) as usize;
    let game_start_offset        = u32::from_be_bytes(slpz[8..12].try_into().unwrap()) as usize;
    let metadata_offset          = u32::from_be_bytes(slpz[12..16].try_into().unwrap()) as usize;
    let compressed_events_offset = u32::from_be_bytes(slpz[16..20].try_into().unwrap()) as usize;
    let decompressed_events_size = u32::from_be_bytes(slpz[20..24].try_into().unwrap()) as usize;

    if slpz.len() < compressed_events_offset { return Err(DecompError::InvalidFile) }
    if version > VERSION { return Err(DecompError::VersionTooNew) }

    let mut slp = Vec::with_capacity(slpz.len() * 32);
    slp.extend_from_slice(&RAW_HEADER);
    slp.extend_from_slice(&[0u8; 4]); // raw len. filled in later

    let event_sizes_bytes = &slpz[event_sizes_offset..game_start_offset];
    slp.extend_from_slice(event_sizes_bytes);
    let (event_sizes, _) = event_sizes(event_sizes_bytes).ok_or(DecompError::InvalidFile)?;
    slp.extend_from_slice(&slpz[game_start_offset..metadata_offset]);

    let b = decompressor.ctx.decompress(&slpz[compressed_events_offset..], decompressed_events_size)
        .map_err(|_| DecompError::DecompressionFailure)?;
    unorder_events(&b, &event_sizes, &mut slp)?;

    let metadata_offset_in_slp = slp.len();
    slp.extend_from_slice(&slpz[metadata_offset..compressed_events_offset]);

    slp[11..15].copy_from_slice(&(metadata_offset_in_slp as u32 - 15).to_be_bytes()); // raw len

    Ok(slp)
}

/// Reorders events into byte columns.
pub fn reorder_events(
    events: &[u8], 
    event_sizes: &[u16; 256],
    buf: &mut Vec<u8>,
) -> CompResult<usize> {
    let event_counts = event_counts(events, event_sizes)?;

    // ---------------------------------------
    // Build the offset lookup table 'reordered_event_offsets'. 
    // This is the offset of the start of the reordered data for each event in the reordered event data section.

    let mut total_events = 0usize;
    let mut reordered_event_offsets = [0u32; 256];

    for i in 0..255 {
        let size = event_sizes[i];
        let count = event_counts[i];
        total_events += count as usize;
        
        let event_total_size = size as u32 * count;

        // offset for next event is the end of this event.
        reordered_event_offsets[i+1] = reordered_event_offsets[i] + event_total_size;
    }

    let reordered_size = {
        let last_size = event_sizes[255];
        let last_count = event_counts[255];
        total_events += last_count as usize;
        let last_total_size = last_count as usize * last_size as usize;

        reordered_event_offsets[255] as usize + last_total_size
    };

    if reordered_size != events.len() - total_events { return Err(CompError::InvalidFile) }

    // alloc
    let data_size = 4 + total_events + reordered_size;
    let buf_prev = buf.len();
    buf.resize(buf_prev + data_size, 0u8);
    let data = &mut buf[buf_prev..];

    // ---------------------------------------
    // fill event order list and reordered data

    data[0..4].copy_from_slice(&(total_events as u32).to_be_bytes());

    let event_order_list_offset = 4;
    let reordered_events_offset = event_order_list_offset + total_events;

    let mut events_written = [0u32; 256];
    let mut event_i = 0;
    let mut i = 0;
    while i < events.len() {
        let event_u8 = events[i];
        let event = event_u8 as usize;

        // fill event order list
        data[event_order_list_offset + event_i] = event_u8;

        // fill reorder data
        let event_offset = reordered_events_offset + reordered_event_offsets[event] as usize;
        let written = events_written[event] as usize;
        let size = event_sizes[event] as usize;
        let stride = event_counts[event] as usize;

        let write_start = event_offset + written;
        for j in 0..size {
            data[write_start + j*stride] = events[1+i+j];
        }

        events_written[event] += 1;

        i += 1 + size;
        event_i += 1;
    }

    Ok(data_size)
}

/// Undoes the reordering done by 'reorder_events'.
///
/// Returns the number of bytes written.
pub fn unorder_events(
    b: &[u8], 
    event_sizes: &[u16; 256], 
    buf: &mut Vec<u8>,
) -> DecompResult<usize> {
    let total_events = u32::from_be_bytes(b[0..4].try_into().unwrap()) as usize;

    let event_order_list_offset = 4;
    let reordered_events_offset = event_order_list_offset + total_events;

    let mut event_counts = [0u32; 256];
    for i in 0..total_events {
        let event = b[event_order_list_offset+i] as usize;
        event_counts[event] += 1;
    }

    let mut reordered_event_offsets = [0u32; 256];
    for i in 0..255 {
        let size = event_sizes[i];
        let count = event_counts[i];
        
        let event_total_size = size as u32 * count;

        // offset for next event is the end of this event.
        reordered_event_offsets[i+1] = reordered_event_offsets[i] + event_total_size;
    }

    let unordered_size = {
        let last_size = event_sizes[255];
        let last_count = event_counts[255];
        let last_total_size = last_count as usize * last_size as usize;
        reordered_event_offsets[255] as usize + last_total_size + total_events
    };

    let event_order_list = &b[event_order_list_offset..reordered_events_offset];
    let events = &b[reordered_events_offset..];

    if unordered_size != events.len() + total_events { return Err(DecompError::InvalidFile) }

    let buf_prev = buf.len();
    buf.resize(buf_prev + unordered_size, 0u8);
    let data = &mut buf[buf_prev..];

    let mut events_written = [0u32; 256];

    let mut data_i = 0;
    for event_i in 0..total_events {
        let event_u8 = event_order_list[event_i];
        let event = event_u8 as usize;

        // command byte
        data[data_i] = event_u8;

        // unorder data
        let event_offset = reordered_event_offsets[event] as usize;
        let written = events_written[event] as usize;
        let size = event_sizes[event] as usize;
        let stride = event_counts[event] as usize;

        let write_start = event_offset + written;
        for j in 0..size {
            data[1+data_i+j] = events[write_start + j*stride];
        }

        events_written[event] += 1;

        data_i += 1 + size;
    }

    Ok(unordered_size)
}

fn event_sizes(events: &[u8]) -> Option<([u16; 256], usize)> {
    if events.is_empty() { return None }

    let info_size = events[1] as usize;
    let event_count = (info_size - 1) / 3;

    if events.len() < info_size { return None }

    let mut event_payload_sizes = [0; 256];
    for i in 0..event_count {
        let offset = i*3 + 2;
        let command_byte = events[offset] as usize;
        let payload_size = u16::from_be_bytes(events[offset+1..][..2].try_into().unwrap());
        event_payload_sizes[command_byte] = payload_size;
    }

    Some((event_payload_sizes, event_count as usize))
}

fn event_counts(events: &[u8], event_sizes: &[u16; 256]) -> CompResult<[u32; 256]> {
    let mut i = 0;
    let mut counts = [0u32; 256];

    while i < events.len() {
        let event = events[i] as usize;
        let event_size = event_sizes[event];
        if event_size == 0 { return Err(CompError::InvalidFile) }
        counts[event] += 1;
        i += 1 + event_size as usize; // skip command byte and payload
    }

    Ok(counts)
}
