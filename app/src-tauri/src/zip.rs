// SPDX-License-Identifier: AGPL-3.0-or-later
//! Минимальная запись ZIP без сжатия (метод «stored») — для архива диагностики.

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Время в формате DOS из строки «ГГГГ-ММ-ДД чч:мм:сс».
fn dos_time(local: &str) -> (u16, u16) {
    let n = |a: usize, b: usize| local.get(a..b).and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
    let (year, month, day) = (n(0, 4).max(1980), n(5, 7).max(1), n(8, 10).max(1));
    let (h, m, s) = (n(11, 13), n(14, 16), n(17, 19));
    ((h << 11) | (m << 5) | (s / 2), ((year - 1980) << 9) | (month << 5) | day)
}

/// Собирает архив из пар (имя файла, содержимое).
pub fn build(files: &[(String, Vec<u8>)], local_time: &str) -> Vec<u8> {
    let (time, date) = dos_time(local_time);
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name = name.as_bytes();
        let common = |v: &mut Vec<u8>| {
            v.extend_from_slice(&20u16.to_le_bytes()); // версия для распаковки
            v.extend_from_slice(&0x0800u16.to_le_bytes()); // имена в UTF-8
            v.extend_from_slice(&0u16.to_le_bytes()); // без сжатия
            v.extend_from_slice(&time.to_le_bytes());
            v.extend_from_slice(&date.to_le_bytes());
            v.extend_from_slice(&crc.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&(name.len() as u16).to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes()); // доп. поле
        };
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        common(&mut out);
        out.extend_from_slice(name);
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // создано версией
        common(&mut central);
        central.extend_from_slice(&[0; 8]); // комментарий, диск, атрибуты
        central.extend_from_slice(&0u32.to_le_bytes()); // внешние атрибуты
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]); // номера дисков
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn archive_has_valid_structure() {
        let files = vec![("a.txt".to_string(), b"hello".to_vec()), ("b/c.json".to_string(), b"{}".to_vec())];
        let z = build(&files, "2026-11-02 14:05:09");
        assert_eq!(&z[..4], &[0x50, 0x4b, 0x03, 0x04]);
        let eocd = &z[z.len() - 22..];
        assert_eq!(&eocd[..4], &[0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes([eocd[10], eocd[11]]), 2);
        let cd_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]) as usize;
        assert_eq!(&z[cd_offset..cd_offset + 4], &[0x50, 0x4b, 0x01, 0x02]);
    }

    #[test]
    fn dos_time_encodes_fields() {
        let (t, d) = dos_time("2026-11-02 14:05:09");
        assert_eq!(t >> 11, 14);
        assert_eq!((t >> 5) & 0x3f, 5);
        assert_eq!(d >> 9, 46);
        assert_eq!((d >> 5) & 0xf, 11);
        assert_eq!(d & 0x1f, 2);
    }
}
