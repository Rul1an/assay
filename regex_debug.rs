use regex::Regex;

fn main() {
    let pattern = r"^/(etc/passwd|etc/shadow|root/|\.ssh/|\.aws/|\.env)";
    let re = Regex::new(pattern).unwrap();

    let path1 = "/etc/passwd";
    let path2 = "etc/passwd";
    let path3 = "/private/etc/passwd"; // MacOS symlink often used

    println!("Pattern: {}", pattern);
    println!("'{}' matches: {}", path1, re.is_match(path1));
    println!("'{}' matches: {}", path2, re.is_match(path2));
    println!("'{}' matches: {}", path3, re.is_match(path3));
}
