// EndBASIC
// Copyright 2021 Julio Merino
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not
// use this file except in compliance with the License.  You may obtain a copy
// of the License at:
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
// License for the specific language governing permissions and limitations
// under the License.

//! Exposes EndBASIC demos as an overlay to the drive.

use endbasic_std::store::{Drive, Metadata};
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::str;

/// Wraps a drive and exposes a bunch of read-only demo files.
///
/// All demo file names are case insensitive.  However, this preserves the case sensitiveness
/// behavior of the underlying drive for any files that are passed through.
///
/// This takes ownership of any file names that start with `DEMO:`, which means any such files in
/// the underlying drive become invisible.  This should not be a problem in practice because most
/// file systems deny the `:` character in file names.
pub struct DemoDriveOverlay<D: Drive> {
    /// The demos to expose, expressed as a mapping of names to (metadata, content) pairs.
    demos: HashMap<&'static str, (Metadata, String)>,

    /// The wrapped drive.
    delegate: D,
}

/// Converts the raw bytes of a demo file into the program string to expose.
///
/// The input `bytes` must be valid UTF-8, which we can guarantee because these bytes come from
/// files that we own in the source tree.
///
/// On Windows, the output string has all CRLF pairs converted to LF to ensure that the reported
/// demo file sizes are consistent across platforms.
fn process_demo(bytes: &[u8]) -> String {
    let raw_content = str::from_utf8(bytes).expect("Malformed demo file");
    if cfg!(target_os = "windows") {
        raw_content.replace("\r\n", "\n")
    } else {
        raw_content.to_owned()
    }
}

impl<D: Drive> DemoDriveOverlay<D> {
    /// Creates a new demo drive that wraps the `delegate` drive.
    pub fn new(delegate: D) -> Self {
        let mut demos = HashMap::default();
        {
            let content = process_demo(include_bytes!("../examples/guess.bas"));
            let metadata = Metadata {
                date: time::OffsetDateTime::from_unix_timestamp(1608693152),
                length: content.len() as u64,
            };
            demos.insert("DEMO:GUESS.BAS", (metadata, content));
        }
        {
            let content = process_demo(include_bytes!("../examples/gpio.bas"));
            let metadata = Metadata {
                date: time::OffsetDateTime::from_unix_timestamp(1613316558),
                length: content.len() as u64,
            };
            demos.insert("DEMO:GPIO.BAS", (metadata, content));
        }
        {
            let content = process_demo(include_bytes!("../examples/hello.bas"));
            let metadata = Metadata {
                date: time::OffsetDateTime::from_unix_timestamp(1608646800),
                length: content.len() as u64,
            };
            demos.insert("DEMO:HELLO.BAS", (metadata, content));
        }
        {
            let content = process_demo(include_bytes!("../examples/tour.bas"));
            let metadata = Metadata {
                date: time::OffsetDateTime::from_unix_timestamp(1608774770),
                length: content.len() as u64,
            };
            demos.insert("DEMO:TOUR.BAS", (metadata, content));
        }
        Self { demos, delegate }
    }

    /// Disowns and returns the underlying delegate drive.
    pub fn unmount(self) -> D {
        self.delegate
    }
}

impl<D: Drive> Drive for DemoDriveOverlay<D> {
    fn delete(&mut self, name: &str) -> io::Result<()> {
        let uc_name = name.to_ascii_uppercase();
        match self.demos.get(&uc_name.as_ref()) {
            Some(_) => {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "Demo files are read-only"))
            }
            _ if uc_name.starts_with("DEMO:") => {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "Demo files are read-only"))
            }
            _ => self.delegate.delete(name),
        }
    }

    fn enumerate(&self) -> io::Result<BTreeMap<String, Metadata>> {
        let mut entries = self.delegate.enumerate()?;

        // TODO(https://github.com/rust-lang/rust/issues/70530): Use drain_filter when available.
        let mut hidden_names = vec![];
        for (name, _) in entries.iter() {
            if name.to_ascii_uppercase().starts_with("DEMO:") {
                hidden_names.push(name.to_owned());
            }
        }
        for name in hidden_names {
            entries.remove(&name);
        }

        for (name, (metadata, _content)) in self.demos.iter() {
            entries.insert(name.to_string(), metadata.clone());
        }
        Ok(entries)
    }

    fn get(&self, name: &str) -> io::Result<String> {
        let uc_name = name.to_ascii_uppercase();
        match self.demos.get(&uc_name.as_ref()) {
            Some(value) => {
                let (_metadata, content) = value;
                Ok(content.to_string())
            }
            _ if uc_name.starts_with("DEMO:") => {
                Err(io::Error::new(io::ErrorKind::NotFound, "Non-existing demo file"))
            }
            _ => self.delegate.get(name),
        }
    }

    fn put(&mut self, name: &str, content: &str) -> io::Result<()> {
        let uc_name = name.to_ascii_uppercase();
        match self.demos.get(&uc_name.as_ref()) {
            Some(_) => {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "Demo files are read-only"))
            }
            _ if uc_name.starts_with("DEMO:") => {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "Demo files are read-only"))
            }
            _ => self.delegate.put(name, content),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use endbasic_std::store::InMemoryDrive;

    #[test]
    fn test_demo_drive_overlay_delete() {
        let mut drive = InMemoryDrive::default();
        drive.put("delete.bas", "underlying file").unwrap();
        drive.put("keep.bas", "underlying file").unwrap();
        drive.put("demo:unknown.bas", "should not be touched").unwrap();
        let drive = {
            let mut drive = DemoDriveOverlay::new(drive);

            drive.delete("delete.bas").unwrap();
            assert_eq!(io::ErrorKind::NotFound, drive.delete("KEEP.Bas").unwrap_err().kind());

            assert_eq!(
                io::ErrorKind::PermissionDenied,
                drive.delete("demo:hello.bas").unwrap_err().kind()
            );
            assert_eq!(
                io::ErrorKind::PermissionDenied,
                drive.delete("DEMO:Hello.BAS").unwrap_err().kind()
            );

            assert_eq!(
                io::ErrorKind::PermissionDenied,
                drive.delete("demo:unknown.bas").unwrap_err().kind()
            );

            drive.unmount()
        };
        assert_eq!(io::ErrorKind::NotFound, drive.get("delete.bas").unwrap_err().kind());
        assert_eq!("underlying file", drive.get("keep.bas").unwrap());
        assert_eq!(io::ErrorKind::NotFound, drive.get("demo:hello.bas").unwrap_err().kind());
        assert_eq!("should not be touched", drive.get("demo:unknown.bas").unwrap());
    }

    #[test]
    fn test_demo_drive_overlay_enumerate() {
        let mut drive = InMemoryDrive::default();
        drive.put("under.bas", "underlying file").unwrap();
        drive.put("demo:hidden.bas", "will be clobbered").unwrap();
        let drive = DemoDriveOverlay::new(drive);

        let entries = drive.enumerate().unwrap();
        assert!(entries.contains_key("under.bas"));
        assert!(entries.contains_key("DEMO:GUESS.BAS"));
        assert!(entries.contains_key("DEMO:HELLO.BAS"));
        assert!(entries.contains_key("DEMO:TOUR.BAS"));
        assert!(!entries.contains_key("DEMO:HIDDEN.BAS"));
        assert!(!entries.contains_key("demo:hidden.bas"));
    }

    #[test]
    fn test_demo_drive_overlay_get() {
        let mut drive = InMemoryDrive::default();
        drive.put("under.bas", "underlying file").unwrap();
        drive.put("demo:hidden.bas", "will be clobbered").unwrap();
        let drive = DemoDriveOverlay::new(drive);

        assert_eq!("underlying file", drive.get("under.bas").unwrap());
        assert_eq!(io::ErrorKind::NotFound, drive.get("Under.bas").unwrap_err().kind());

        assert_eq!(
            process_demo(include_bytes!("../examples/hello.bas")),
            drive.get("demo:hello.bas").unwrap()
        );
        assert_eq!(
            process_demo(include_bytes!("../examples/hello.bas")),
            drive.get("Demo:Hello.Bas").unwrap()
        );

        assert_eq!(io::ErrorKind::NotFound, drive.get("demo:hidden.bas").unwrap_err().kind());
        assert_eq!(io::ErrorKind::NotFound, drive.get("demo:unknown.bas").unwrap_err().kind());
        assert_eq!(io::ErrorKind::NotFound, drive.get("unknown.bas").unwrap_err().kind());
    }

    #[test]
    fn test_demo_drive_overlay_put() {
        let mut drive = InMemoryDrive::default();
        drive.put("modify.bas", "previous contents").unwrap();
        drive.put("avoid.bas", "previous contents").unwrap();
        let drive = {
            let mut drive = DemoDriveOverlay::new(drive);

            drive.put("modify.bas", "new contents").unwrap();

            assert_eq!(
                io::ErrorKind::PermissionDenied,
                drive.put("demo:hello.bas", "").unwrap_err().kind()
            );
            assert_eq!(
                io::ErrorKind::PermissionDenied,
                drive.put("DEMO:Hello.BAS", "").unwrap_err().kind()
            );

            assert_eq!(
                io::ErrorKind::PermissionDenied,
                drive.put("demo:unknown.bas", "").unwrap_err().kind()
            );

            drive.unmount()
        };
        assert_eq!(io::ErrorKind::NotFound, drive.get("demo:unknown.bas").unwrap_err().kind());
        assert_eq!("new contents", drive.get("modify.bas").unwrap());
        assert_eq!("previous contents", drive.get("avoid.bas").unwrap());
    }
}
