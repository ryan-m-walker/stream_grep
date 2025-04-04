pub struct State {
    pub outpub_buffer: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            outpub_buffer: Vec::new(),
        }
    }
}
