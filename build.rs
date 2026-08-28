fn main() {
    #[cfg(windows)]
    {
        let _ = embed_resource::compile("water-remainder.rc", embed_resource::NONE);
    }
}
