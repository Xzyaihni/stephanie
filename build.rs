use std::fs;

fn main()
{
	let _ = fs::remove_dir_all("worlds");
	println!("cargo:rerun-if-changed=src");
}