use paste_protocol::{HelperStage, PROTOCOL_VERSION};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

const MAGIC: &[u8; 8] = b"ZCLIPTXN";
const MAX_FILE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug)]
pub struct ClipboardFormat {
    pub format_id: u32,
    pub registered_name: Option<String>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub transaction_id: Uuid,
    pub captured_sequence: u32,
    pub phase: Option<HelperStage>,
    pub payload_sequence: Option<u32>,
    pub payload_sha256: Option<[u8; 32]>,
    pub formats: Vec<ClipboardFormat>,
}

pub fn snapshot_directory() -> Result<PathBuf, String> {
    let root = dirs::data_local_dir().ok_or_else(|| "cannot resolve LocalAppData".to_string())?;
    let directory = root.join("gy-typing").join("clipboard-transactions");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

pub fn snapshot_path(transaction_id: Uuid) -> Result<PathBuf, String> {
    Ok(snapshot_directory()?.join(format!("{transaction_id}.ztxn")))
}

pub fn write(snapshot: &Snapshot) -> Result<(), String> {
    write_to_path(snapshot, &snapshot_path(snapshot.transaction_id)?)
}

fn write_to_path(snapshot: &Snapshot, final_path: &Path) -> Result<(), String> {
    let clear = encode(snapshot)?;
    let encrypted = protect(&clear)?;
    let temporary = final_path.with_extension("tmp");
    let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
    file.write_all(&encrypted)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    atomic_replace(&temporary, final_path)
}

pub fn read(transaction_id: Uuid) -> Result<Snapshot, String> {
    read_from_path(&snapshot_path(transaction_id)?)
}

fn read_from_path(path: &Path) -> Result<Snapshot, String> {
    let encrypted = fs::read(path).map_err(|error| error.to_string())?;
    decode(&unprotect(&encrypted)?)
}

pub fn remove(transaction_id: Uuid) {
    if let Ok(path) = snapshot_path(transaction_id) {
        let _ = fs::remove_file(path);
    }
}

pub fn cleanup_expired() -> Result<(), String> {
    let now = SystemTime::now();
    for entry in fs::read_dir(snapshot_directory()?).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !is_transaction_file(&path) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if now.duration_since(modified).unwrap_or(MAX_FILE_AGE) >= MAX_FILE_AGE {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

pub fn self_check() -> Result<(), String> {
    let transaction_id = Uuid::new_v4();
    let snapshot = Snapshot {
        transaction_id,
        captured_sequence: 0,
        phase: None,
        payload_sequence: None,
        payload_sha256: None,
        formats: Vec::new(),
    };
    let result = (|| {
        write(&snapshot)?;
        let restored = read(transaction_id)?;
        if restored.transaction_id != transaction_id || !restored.formats.is_empty() {
            return Err("snapshot self-check round trip mismatch".to_string());
        }
        Ok(())
    })();
    remove(transaction_id);
    result
}

fn is_transaction_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("ztxn" | "tmp")
    ) && path
        .file_stem()
        .and_then(|value| value.to_str())
        .and_then(|value| Uuid::parse_str(value).ok())
        .is_some()
}

#[cfg(target_os = "windows")]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| error.to_string())
}

fn encode(snapshot: &Snapshot) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
    bytes.extend_from_slice(snapshot.transaction_id.as_bytes());
    bytes.extend_from_slice(&snapshot.captured_sequence.to_le_bytes());
    bytes.push(stage_byte(snapshot.phase));
    match snapshot.payload_sequence {
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        None => bytes.push(0),
    }
    match snapshot.payload_sha256 {
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value);
        }
        None => bytes.push(0),
    }
    let count = u32::try_from(snapshot.formats.len()).map_err(|_| "too many formats")?;
    bytes.extend_from_slice(&count.to_le_bytes());
    for format in &snapshot.formats {
        bytes.extend_from_slice(&format.format_id.to_le_bytes());
        let name = format.registered_name.as_deref().unwrap_or("").as_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| "format name too long")?;
        bytes.extend_from_slice(&name_len.to_le_bytes());
        bytes.extend_from_slice(name);
        let data_len = u64::try_from(format.data.len()).map_err(|_| "format data too long")?;
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.extend_from_slice(&format.data);
    }
    let digest = Sha256::digest(&bytes);
    bytes.extend_from_slice(&digest);
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<Snapshot, String> {
    if bytes.len() < MAGIC.len() + 2 + 16 + 4 + 1 + 1 + 1 + 4 + 32 {
        return Err("snapshot is truncated".to_string());
    }
    let (body, expected_digest) = bytes.split_at(bytes.len() - 32);
    if Sha256::digest(body).as_slice() != expected_digest {
        return Err("snapshot integrity mismatch".to_string());
    }
    let mut cursor = Cursor::new(body);
    let mut magic = [0u8; 8];
    cursor
        .read_exact(&mut magic)
        .map_err(|error| error.to_string())?;
    if &magic != MAGIC {
        return Err("snapshot magic mismatch".to_string());
    }
    let version = read_u16(&mut cursor)?;
    if version != PROTOCOL_VERSION {
        return Err("snapshot protocol mismatch".to_string());
    }
    let mut uuid = [0u8; 16];
    cursor
        .read_exact(&mut uuid)
        .map_err(|error| error.to_string())?;
    let transaction_id = Uuid::from_bytes(uuid);
    let captured_sequence = read_u32(&mut cursor)?;
    let phase = byte_stage(read_u8(&mut cursor)?)?;
    let payload_sequence = if read_u8(&mut cursor)? == 1 {
        Some(read_u32(&mut cursor)?)
    } else {
        None
    };
    let payload_sha256 = if read_u8(&mut cursor)? == 1 {
        let mut digest = [0u8; 32];
        cursor
            .read_exact(&mut digest)
            .map_err(|error| error.to_string())?;
        Some(digest)
    } else {
        None
    };
    let count = read_u32(&mut cursor)? as usize;
    if count > 256 {
        return Err("snapshot format count exceeds limit".to_string());
    }
    let mut formats = Vec::with_capacity(count);
    let mut total = 0usize;
    for _ in 0..count {
        let format_id = read_u32(&mut cursor)?;
        let name_len = read_u16(&mut cursor)? as usize;
        let mut name = vec![0u8; name_len];
        cursor
            .read_exact(&mut name)
            .map_err(|error| error.to_string())?;
        let data_len = usize::try_from(read_u64(&mut cursor)?).map_err(|_| "format too large")?;
        total = total
            .checked_add(data_len)
            .ok_or("snapshot size overflow")?;
        if data_len > 64 * 1024 * 1024 || total > 128 * 1024 * 1024 {
            return Err("snapshot size exceeds limit".to_string());
        }
        let mut data = vec![0u8; data_len];
        cursor
            .read_exact(&mut data)
            .map_err(|error| error.to_string())?;
        formats.push(ClipboardFormat {
            format_id,
            registered_name: if name.is_empty() {
                None
            } else {
                Some(String::from_utf8(name).map_err(|_| "invalid format name")?)
            },
            data,
        });
    }
    if cursor.position() != body.len() as u64 {
        return Err("snapshot has trailing data".to_string());
    }
    Ok(Snapshot {
        transaction_id,
        captured_sequence,
        phase,
        payload_sequence,
        payload_sha256,
        formats,
    })
}

fn stage_byte(stage: Option<HelperStage>) -> u8 {
    match stage {
        None => 0,
        Some(HelperStage::SnapshotComplete) => 1,
        Some(HelperStage::PayloadWriteStarted) => 2,
        Some(HelperStage::PayloadWritten) => 3,
        Some(HelperStage::TargetVerified) => 4,
        Some(HelperStage::PasteSubmitting) => 5,
        Some(HelperStage::PasteSubmitted) => 6,
        Some(HelperStage::RestoreStarted) => 7,
    }
}

fn byte_stage(value: u8) -> Result<Option<HelperStage>, String> {
    Ok(match value {
        0 => None,
        1 => Some(HelperStage::SnapshotComplete),
        2 => Some(HelperStage::PayloadWriteStarted),
        3 => Some(HelperStage::PayloadWritten),
        4 => Some(HelperStage::TargetVerified),
        5 => Some(HelperStage::PasteSubmitting),
        6 => Some(HelperStage::PasteSubmitted),
        7 => Some(HelperStage::RestoreStarted),
        _ => return Err("invalid snapshot stage".to_string()),
    })
}

fn read_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8, String> {
    let mut bytes = [0u8; 1];
    cursor
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes[0])
}

fn read_u16(cursor: &mut Cursor<&[u8]>) -> Result<u16, String> {
    let mut bytes = [0u8; 2];
    cursor
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32, String> {
    let mut bytes = [0u8; 4];
    cursor
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(cursor: &mut Cursor<&[u8]>) -> Result<u64, String> {
    let mut bytes = [0u8; 8];
    cursor
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(target_os = "windows")]
fn protect(clear: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(clear.len()).map_err(|_| "snapshot too large")?,
        pbData: clear.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptProtectData(&input, None, None, None, None, 0, &mut output) }
        .map_err(|error| error.to_string())?;
    let encrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(HLOCAL(output.pbData.cast())) };
    Ok(encrypted)
}

#[cfg(target_os = "windows")]
fn unprotect(encrypted: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(encrypted.len()).map_err(|_| "snapshot too large")?,
        pbData: encrypted.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptUnprotectData(&input, None, None, None, None, 0, &mut output) }
        .map_err(|error| error.to_string())?;
    let clear =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(HLOCAL(output.pbData.cast())) };
    Ok(clear)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_binary_round_trips_and_detects_tampering() {
        let snapshot = Snapshot {
            transaction_id: Uuid::nil(),
            captured_sequence: 12,
            phase: Some(HelperStage::PayloadWritten),
            payload_sequence: Some(15),
            payload_sha256: Some([7; 32]),
            formats: vec![ClipboardFormat {
                format_id: 13,
                registered_name: None,
                data: vec![65, 0, 0, 0],
            }],
        };
        let encoded = encode(&snapshot).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.transaction_id, snapshot.transaction_id);
        assert_eq!(decoded.formats[0].data, snapshot.formats[0].data);
        let mut tampered = encoded;
        tampered[10] ^= 1;
        assert!(decode(&tampered).is_err());
    }

    #[test]
    fn encrypted_snapshot_can_be_atomically_replaced_across_stages() {
        let transaction_id = Uuid::new_v4();
        let path = std::env::temp_dir().join(format!("gy-typing-{transaction_id}.ztxn"));
        let mut snapshot = Snapshot {
            transaction_id,
            captured_sequence: 21,
            phase: Some(HelperStage::SnapshotComplete),
            payload_sequence: None,
            payload_sha256: None,
            formats: vec![ClipboardFormat {
                format_id: 13,
                registered_name: None,
                data: vec![65, 0, 0, 0],
            }],
        };
        write_to_path(&snapshot, &path).unwrap();
        snapshot.phase = Some(HelperStage::PayloadWritten);
        snapshot.payload_sequence = Some(22);
        snapshot.payload_sha256 = Some([9; 32]);
        write_to_path(&snapshot, &path).unwrap();
        let restored = read_from_path(&path).unwrap();
        assert_eq!(restored.phase, Some(HelperStage::PayloadWritten));
        assert_eq!(restored.payload_sequence, Some(22));
        std::fs::remove_file(&path).unwrap();
        assert!(!path.exists());
    }
}
