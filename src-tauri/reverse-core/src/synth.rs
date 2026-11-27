//! Synthetic PE builder: used by unit tests, by the rule self-tests / recall evaluation and by the
//! packed-file fixtures. Builds minimal VALID PE32+ images so the real parser is exercised end to end.

/// Build a minimal valid PE32+ with the given sections `(name, data, characteristics)` and an
/// optional import table `(dll, [function names])`.
pub fn build_pe(sections: &[(&str, Vec<u8>, u32)], imports: &[(&str, &[&str])], is_dll: bool) -> Vec<u8> {
    let file_align = 0x200usize;
    let sect_align = 0x1000usize;
    let pe_off = 0x80usize;
    let opt_size = 0xF0usize; // PE32+ optional header with 16 data directories
    let headers_end = pe_off + 4 + 20 + opt_size + 40 * (sections.len() + 1);
    let headers_size = (headers_end + file_align - 1) / file_align * file_align;

    // Layout: user sections first, then an .idata section if imports were requested.
    let mut secs: Vec<(String, Vec<u8>, u32)> = sections.iter().map(|(n, d, c)| (n.to_string(), d.clone(), *c)).collect();
    let idata_index = if imports.is_empty() { None } else { Some(secs.len()) };
    if idata_index.is_some() {
        secs.push((".idata".to_string(), Vec::new(), 0xC000_0040));
    }
    // virtual addresses
    let mut vas = Vec::new();
    let mut va = sect_align;
    for (_, d, _) in &secs {
        vas.push(va);
        va += (d.len().max(0x200) + sect_align - 1) / sect_align * sect_align;
    }
    // build import data now that we know .idata's RVA
    let mut import_dir_rva = 0u32;
    let mut import_dir_size = 0u32;
    if let Some(ix) = idata_index {
        let base = vas[ix] as u32;
        let n = imports.len();
        let desc_size = (n + 1) * 20;
        let mut blob = vec![0u8; desc_size];
        let mut cursor = desc_size;
        for (k, (dll, funcs)) in imports.iter().enumerate() {
            let dll_off = cursor;
            blob.extend_from_slice(dll.as_bytes());
            blob.push(0);
            cursor = blob.len();
            // hint/name entries
            let mut hn_offsets = Vec::new();
            for f in funcs.iter() {
                if cursor % 2 == 1 {
                    blob.push(0);
                    cursor += 1;
                }
                hn_offsets.push(cursor);
                blob.extend_from_slice(&[0, 0]);
                blob.extend_from_slice(f.as_bytes());
                blob.push(0);
                cursor = blob.len();
            }
            while blob.len() % 8 != 0 {
                blob.push(0);
            }
            let ilt_off = blob.len();
            for off in &hn_offsets {
                blob.extend_from_slice(&((base as u64 + *off as u64).to_le_bytes()));
            }
            blob.extend_from_slice(&0u64.to_le_bytes());
            let iat_off = blob.len();
            for off in &hn_offsets {
                blob.extend_from_slice(&((base as u64 + *off as u64).to_le_bytes()));
            }
            blob.extend_from_slice(&0u64.to_le_bytes());
            cursor = blob.len();
            let d = k * 20;
            blob[d..d + 4].copy_from_slice(&(base + ilt_off as u32).to_le_bytes()); // OriginalFirstThunk
            blob[d + 12..d + 16].copy_from_slice(&(base + dll_off as u32).to_le_bytes()); // Name
            blob[d + 16..d + 20].copy_from_slice(&(base + iat_off as u32).to_le_bytes()); // FirstThunk
        }
        import_dir_rva = base;
        import_dir_size = desc_size as u32;
        secs[ix].1 = blob;
    }
    // file offsets
    let mut raw_offs = Vec::new();
    let mut off = headers_size;
    for (_, d, _) in &secs {
        raw_offs.push(off);
        off += (d.len() + file_align - 1) / file_align * file_align;
    }
    let size_of_image = va;
    let mut out = vec![0u8; off];
    out[0] = b'M';
    out[1] = b'Z';
    out[0x3c..0x40].copy_from_slice(&(pe_off as u32).to_le_bytes());
    let mut p = pe_off;
    out[p..p + 4].copy_from_slice(b"PE\0\0");
    p += 4;
    out[p..p + 2].copy_from_slice(&0x8664u16.to_le_bytes()); // machine
    out[p + 2..p + 4].copy_from_slice(&(secs.len() as u16).to_le_bytes());
    out[p + 4..p + 8].copy_from_slice(&0x5f00_0000u32.to_le_bytes()); // timestamp
    out[p + 16..p + 18].copy_from_slice(&(opt_size as u16).to_le_bytes());
    let chars: u16 = 0x0022 | if is_dll { 0x2000 } else { 0 };
    out[p + 18..p + 20].copy_from_slice(&chars.to_le_bytes());
    p += 20;
    out[p..p + 2].copy_from_slice(&0x20bu16.to_le_bytes()); // PE32+
    out[p + 2] = 14;
    out[p + 16..p + 20].copy_from_slice(&0x1000u32.to_le_bytes()); // entry point
    out[p + 24..p + 32].copy_from_slice(&0x1_4000_0000u64.to_le_bytes()); // image base
    out[p + 32..p + 36].copy_from_slice(&(sect_align as u32).to_le_bytes());
    out[p + 36..p + 40].copy_from_slice(&(file_align as u32).to_le_bytes());
    out[p + 40..p + 42].copy_from_slice(&6u16.to_le_bytes()); // os major
    out[p + 48..p + 50].copy_from_slice(&6u16.to_le_bytes()); // subsystem major
    out[p + 56..p + 60].copy_from_slice(&(size_of_image as u32).to_le_bytes());
    out[p + 60..p + 64].copy_from_slice(&(headers_size as u32).to_le_bytes());
    out[p + 68..p + 70].copy_from_slice(&3u16.to_le_bytes()); // subsystem: console
    out[p + 70..p + 72].copy_from_slice(&0x8160u16.to_le_bytes()); // dll characteristics
    out[p + 108..p + 112].copy_from_slice(&16u32.to_le_bytes()); // number of data directories
    // data directory[1] = import
    out[p + 112 + 8..p + 112 + 12].copy_from_slice(&import_dir_rva.to_le_bytes());
    out[p + 112 + 12..p + 112 + 16].copy_from_slice(&import_dir_size.to_le_bytes());
    p += opt_size;
    for (i, (name, data, ch)) in secs.iter().enumerate() {
        let mut nm = [0u8; 8];
        let nb = name.as_bytes();
        nm[..nb.len().min(8)].copy_from_slice(&nb[..nb.len().min(8)]);
        out[p..p + 8].copy_from_slice(&nm);
        out[p + 8..p + 12].copy_from_slice(&(data.len().max(1) as u32).to_le_bytes());
        out[p + 12..p + 16].copy_from_slice(&(vas[i] as u32).to_le_bytes());
        let raw_size = (data.len() + file_align - 1) / file_align * file_align;
        out[p + 16..p + 20].copy_from_slice(&(raw_size as u32).to_le_bytes());
        out[p + 20..p + 24].copy_from_slice(&(raw_offs[i] as u32).to_le_bytes());
        out[p + 36..p + 40].copy_from_slice(&ch.to_le_bytes());
        out[raw_offs[i]..raw_offs[i] + data.len()].copy_from_slice(data);
        p += 40;
    }
    out
}