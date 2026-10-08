use std::fs;
use std::io;

fn read_proc_stat() -> io::Result<String> {
    let path = "/proc/stat";
    let stat = fs::read_to_string(&path)?;
    // Arguments are null-separated; convert to spaces
    Ok(stat.replace('\0', " "))
}

fn read_proc_meminfo() -> io::Result<String> {
    let path = "/proc/meminfo";
    let meminfo = fs::read_to_string(&path)?;
    // Arguments are null-separated; convert to spaces
    Ok(meminfo.replace('\0', " "))
}

fn main() -> io::Result<()> {
    let pid = std::process::id();
    
    // Read command line
    let stat = read_proc_stat()?;
    println!("Process stat: {}", stat);
    let meminfo = read_proc_meminfo()?;
    println!("Process meminfo: {}", meminfo);
        
    Ok(())
}