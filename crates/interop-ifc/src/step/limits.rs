/// Bounds that keep a hostile file from exhausting memory or the stack.
///
/// Together they bound the memory used for one instance; [`max_instances`](Self::max_instances)
/// bounds the set of instance names kept to detect duplicates. The defaults accept every real IFC
/// model we know of; raise them for exceptional files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Longest token in bytes: a string or binary literal (without its delimiters), a keyword, an
    /// enumeration or a number.
    pub max_token_len: usize,
    /// Longest instance or header entry in bytes, from its first token to its last, comments
    /// included.
    pub max_instance_len: usize,
    /// Deepest nesting of lists and typed parameters; an instance's own parameter list is level 1.
    ///
    /// Parsed values are recursive, so very large values (thousands) risk exhausting the stack
    /// when the values are dropped or walked.
    pub max_nesting_depth: usize,
    /// Most instances in the file. Header entries count against it as well.
    pub max_instances: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_token_len: 4 * 1024 * 1024,
            max_instance_len: 16 * 1024 * 1024,
            max_nesting_depth: 32,
            max_instances: 50_000_000,
        }
    }
}
