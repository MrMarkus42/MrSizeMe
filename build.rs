//! Windows: puts the program icon and the version into the program file.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/mrsizeme.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo"));
    let version = std::env::var("CARGO_PKG_VERSION").expect("set by cargo");
    let mut nums: Vec<u16> = version.split(['.', '-']).filter_map(|p| p.parse().ok()).collect();
    nums.resize(4, 0);
    let icon = root.join("assets/icon/mrsizeme.ico").display().to_string().replace('\\', "/");
    let rc = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {a},{b},{c},{d}
PRODUCTVERSION {a},{b},{c},{d}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Mr. M"
      VALUE "FileDescription", "MrSizeMe"
      VALUE "FileVersion", "{version}"
      VALUE "LegalCopyright", "Mr. M, MIT or Apache-2.0"
      VALUE "OriginalFilename", "mrsizeme.exe"
      VALUE "ProductName", "MrSizeMe"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        a = nums[0],
        b = nums[1],
        c = nums[2],
        d = nums[3],
    );
    let file = out.join("mrsizeme.rc");
    std::fs::write(&file, rc).expect("write resource file");
    embed_resource::compile(&file, embed_resource::NONE).manifest_optional().expect("compile resources");
}
