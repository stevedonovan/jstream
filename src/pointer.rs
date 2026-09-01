/// Return decoded tokens from a JSON Pointer path.
pub(crate) fn pointer_tokens(pointer: &str) -> impl Iterator<Item = String> + '_ {
    pointer.split('/').skip(1).map(unescape_token)
}

fn unescape_token(token: &str) -> String {
    let mut unescaped = String::new();
    let mut chars = token.chars();

    while let Some(ch) = chars.next() {
        if ch != '~' {
            unescaped.push(ch);
            continue;
        }

        match chars.next() {
            Some('0') => unescaped.push('~'),
            Some('1') => unescaped.push('/'),
            Some(ch) => {
                unescaped.push('~');
                unescaped.push(ch);
            }
            None => unescaped.push('~'),
        }
    }

    unescaped
}
