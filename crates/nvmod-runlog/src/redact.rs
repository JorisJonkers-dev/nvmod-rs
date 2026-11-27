// SPDX-License-Identifier: GPL-3.0-only
//! The redaction every line of the run log passes through at the writer (ADR-0012, item 3). Structural redaction
//! ([`crate::Secret`], no bodies, no contents) comes first; this is the net under it, and a best effort.
//!
//! In order: known folders become placeholders (the Instance folder `$INSTANCE`, other configured library paths a
//! named placeholder, the home directory `~`); any other absolute path becomes `<path>`; the value after a
//! `token`, `password`, `secret`, `api_key`, `apikey` (the Nexus API key header, an `apikey=` pair, `NEXUS_API_KEY=`), `authorization` or `bearer`
//! word is masked; the value of a `key` query parameter (an `nxm://` link) is masked; and key-like words are
//! masked (known token prefixes, and long mixed letter-and-digit words that are not a hash).

use std::path::Path;

/// What an unknown absolute path becomes.
pub const PATH_MASK: &str = "<path>";
/// What a masked key becomes.
pub const KEY_MASK: &str = "[redacted]";

const SECRET_WORDS: [&str; 7] = [
    "token",
    "password",
    "passwd",
    "secret",
    "api_key",
    "apikey",
    "authorization",
];
const KEY_PREFIXES: [&str; 8] = [
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "sk-",
    "xoxb-",
    "xoxp-",
];
/// A word at least this long that mixes letters and digits and is not hexadecimal looks like a key.
const KEY_LIKE_LEN: usize = 24;

/// Maps folders to placeholders and masks what looks like a key.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    folders: Vec<(String, String)>,
}

impl Redactor {
    /// A redactor that only does the pattern work.
    #[must_use]
    pub fn new() -> Redactor {
        Redactor::default()
    }

    /// Add the home directory of the current user (`HOME`, or `USERPROFILE` on Windows) as `~`.
    #[must_use]
    pub fn with_home(self) -> Redactor {
        let home = ["HOME", "USERPROFILE"]
            .iter()
            .find_map(|n| std::env::var(n).ok().filter(|v| v.len() > 1));
        match home {
            Some(h) => self.with_folder(Path::new(&h), "~"),
            None => self,
        }
    }

    /// Replace `folder` (and what is below it) with `placeholder`. Empty and root folders are ignored.
    #[must_use]
    pub fn with_folder(mut self, folder: &Path, placeholder: &str) -> Redactor {
        let text = folder.to_string_lossy();
        let text = text.trim_end_matches(['/', '\\']);
        if text.len() > 1 {
            self.folders.push((text.to_owned(), placeholder.to_owned()));
            // Windows takes either slash, and a program may print `C:/Users/me` for `C:\Users\me`.
            if text.contains('\\') {
                self.folders
                    .push((text.replace('\\', "/"), placeholder.to_owned()));
            }
            // Longest first, so `$INSTANCE` wins over `~` for a folder below the home directory.
            self.folders
                .sort_by_key(|(f, _)| std::cmp::Reverse(f.len()));
        }
        self
    }

    /// `text` with folders, paths, secrets and keys replaced.
    #[must_use]
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for (folder, placeholder) in &self.folders {
            out = replace_folder(&out, folder, placeholder);
        }
        let out = mask_paths(&out);
        let out = mask_secret_values(&out);
        let out = mask_query_keys(&out);
        mask_keys(&out)
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// Replace `folder` where it ends at a path boundary (`/home/me` is not in `/home/mentor`).
fn replace_folder(text: &str, folder: &str, placeholder: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(folder) {
        let after = &rest[at + folder.len()..];
        let boundary = after.chars().next().is_none_or(|c| !is_word_char(c));
        out.push_str(&rest[..at]);
        if boundary {
            out.push_str(placeholder);
        } else {
            out.push_str(folder);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// A character that can start a path inside prose or a `key=value` pair.
fn path_may_start_after(prev: Option<char>) -> bool {
    prev.is_none_or(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '(' | '=' | '[' | ',' | ';' | '<' | '>' | '{'
            )
    })
}

fn ends_path(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | ')' | ',' | ';' | '<' | '>' | ']' | '}' | ':' | '`'
        )
}

/// Replace every absolute path that is left (a `/…` path; `C:\…` and `C:/…`; a UNC path `\\server\…`, and the
/// verbatim forms `\\?\C:\…` and `\\?\UNC\…` of Windows) with [`PATH_MASK`].
fn mask_paths(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let prev = i.checked_sub(1).map(|p| chars[p]);
        let c = chars[i];
        let unix = c == '/'
            && (path_may_start_after(prev)
                || (prev == Some(':') && chars.get(i + 1) != Some(&'/')))
            && chars
                .get(i + 1)
                .is_some_and(|n| !n.is_whitespace() && *n != '/');
        let windows = c.is_ascii_alphabetic()
            && path_may_start_after(prev)
            && chars.get(i + 1) == Some(&':')
            && matches!(chars.get(i + 2), Some('\\' | '/'));
        // `\\server\share`, and `//server/share`, which Windows takes too.
        let unc = matches!(c, '\\' | '/')
            && chars.get(i + 1) == Some(&c)
            && path_may_start_after(prev)
            && chars
                .get(i + 2)
                .is_some_and(|n| !n.is_whitespace() && !matches!(n, '\\' | '/'));
        if unix || windows || unc {
            // Skip the drive's colon before looking for the end, also in a verbatim `\\?\C:` or `\\.\C:`.
            let verbatim_drive = unc
                && matches!(chars.get(i + 2), Some('?' | '.'))
                && chars.get(i + 3) == Some(&c)
                && chars.get(i + 4).is_some_and(char::is_ascii_alphabetic)
                && chars.get(i + 5) == Some(&':');
            let mut j = if verbatim_drive {
                i + 6
            } else if windows || unc {
                i + 2
            } else {
                i + 1
            };
            // A quoted path may hold spaces: it runs to the matching quote.
            let quote = prev.filter(|q| matches!(q, '"' | '\''));
            // A UNC path runs on past a colon too (`\\host:8080\x` is not a drive and must not leave its rest).
            let ends = |ch: char| ends_path(ch) && !(unc && ch == ':');
            while j < chars.len() && quote.map_or(!ends(chars[j]), |q| chars[j] != q) {
                j += 1;
            }
            out.push_str(PATH_MASK);
            i = j;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Mask the word that follows `token`, `password`, `bearer` and the like (after `=`, `:` or a space).
fn mask_secret_values(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    while pos < text.len() {
        let hit = SECRET_WORDS
            .iter()
            .chain(["bearer"].iter())
            .filter_map(|w| lower[pos..].find(w).map(|at| (pos + at, w.len())))
            .min();
        let Some((at, len)) = hit else { break };
        let mut end = at + len;
        out.push_str(&text[pos..end]);
        // Separator: optional quote, `=`, `:` and spaces.
        let sep: String = text[end..]
            .chars()
            .take_while(|c| matches!(c, '=' | ':' | ' ' | '"' | '\'' | '\t'))
            .collect();
        // A secret word with no separator (`tokens`, `secretary`) is not an assignment; `bearer` needs a space.
        let separated = sep.chars().any(|c| matches!(c, '=' | ':' | ' '));
        end += sep.len();
        let word_len = |from: usize| {
            text[from..]
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '&'))
                .unwrap_or(text.len() - from)
        };
        let mut value_len = word_len(end);
        if text[end..end + value_len].eq_ignore_ascii_case("bearer") {
            // `Authorization: Bearer <token>`: the token is the next word.
            let gap = text[end + value_len..].len()
                - text[end + value_len..].trim_start_matches(' ').len();
            value_len += gap + word_len(end + value_len + gap);
        }
        if separated
            && value_len > 0
            && !text[end..].starts_with(KEY_MASK)
            && !text[end..].starts_with(PATH_MASK)
        {
            out.push_str(&text[at + len..end]);
            out.push_str(KEY_MASK);
            end += value_len;
        } else {
            out.push_str(&text[at + len..end]);
        }
        pos = end;
    }
    out.push_str(&text[pos..]);
    out
}

/// Mask the value of a `key` query parameter (`?key=` or `&key=`, any case): the one-time download key of an `nxm://`
/// link. The `apikey` parameter is covered by [`mask_secret_values`].
fn mask_query_keys(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    while let Some(at) = ["?key=", "&key="]
        .iter()
        .filter_map(|p| lower[pos..].find(p).map(|a| pos + a))
        .min()
    {
        let start = at + "?key=".len();
        out.push_str(&text[pos..start]);
        let len = text[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '&' | '#'))
            .unwrap_or(text.len() - start);
        if len > 0 && !text[start..].starts_with(KEY_MASK) {
            out.push_str(KEY_MASK);
            pos = start + len;
        } else {
            pos = start;
        }
    }
    out.push_str(&text[pos..]);
    out
}

fn looks_like_key(word: &str) -> bool {
    if KEY_PREFIXES.iter().any(|p| word.starts_with(p)) && word.len() >= 10 {
        return true;
    }
    let hex = word.chars().all(|c| c.is_ascii_hexdigit());
    word.len() >= KEY_LIKE_LEN
        && !hex
        && word.chars().any(|c| c.is_ascii_digit())
        && word.chars().any(|c| c.is_ascii_alphabetic())
}

/// Mask key-like words (runs of letters, digits, `_` and `-`). Hex words (hashes) stay: ADR-0012 logs hashes.
fn mask_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if looks_like_key(word) {
            out.push_str(KEY_MASK);
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        Redactor::new()
            .with_folder(Path::new("/home/me"), "~")
            .with_folder(Path::new("/home/me/games/inst"), "$INSTANCE")
            .with_folder(Path::new("/mnt/steam"), "$BASE")
    }

    #[test]
    fn folders_become_placeholders_longest_first_and_only_at_a_boundary() {
        let r = redactor();
        assert_eq!(
            r.apply("wrote /home/me/games/inst/mods/a.txt"),
            "wrote $INSTANCE/mods/a.txt"
        );
        assert_eq!(r.apply("in /home/me/.config/x"), "in ~/.config/x");
        assert_eq!(r.apply("at /mnt/steam/Fallout"), "at $BASE/Fallout");
        assert_eq!(r.apply("/home/mentor/x"), "<path>");
    }

    #[test]
    fn other_absolute_paths_are_masked_but_urls_and_ratios_stay() {
        let r = redactor();
        assert_eq!(r.apply("open '/data/x y/z' failed"), "open '<path>' failed");
        assert_eq!(r.apply("PATH=/usr/bin:/opt/x"), "PATH=<path>:<path>");
        assert_eq!(
            r.apply(r"cannot read D:\Games\FNV\x.esm now"),
            "cannot read <path> now"
        );
        assert_eq!(
            r.apply("fetch https://github.com/a/b"),
            "fetch https://github.com/a/b"
        );
        assert_eq!(r.apply("3/4 done and/or more"), "3/4 done and/or more");
    }

    #[test]
    fn unc_and_verbatim_windows_paths_are_masked_whole() {
        let r = redactor();
        for path in [
            r"\\server\share\me\x.esm",
            r"\\?\C:\Users\me\x.esm",
            r"\\?\UNC\server\share\x",
            r"\\.\C:\Users\me",
            r"\\host:8080\share\me",
            "//nas/games/me",
            "//?/C:/Users/me",
        ] {
            assert_eq!(
                r.apply(&format!("open {path} failed")),
                "open <path> failed"
            );
            assert_eq!(r.apply(&format!("path=\"{path}\"")), "path=\"<path>\"");
        }
        assert_eq!(
            r.apply(r"a \\ b"),
            r"a \\ b",
            "a lone pair of backslashes stays"
        );
    }

    #[test]
    fn a_windows_folder_is_found_with_either_slash() {
        let r = Redactor::new()
            .with_folder(Path::new(r"C:\Users\me"), "~")
            .with_folder(Path::new(r"C:\Users\me\games\inst"), "$INSTANCE");
        assert_eq!(
            r.apply(r"wrote C:\Users\me\games\inst\mods\a"),
            r"wrote $INSTANCE\mods\a"
        );
        assert_eq!(
            r.apply("wrote C:/Users/me/games/inst/mods/a"),
            "wrote $INSTANCE/mods/a"
        );
        assert_eq!(r.apply("in C:/Users/me/cfg"), "in ~/cfg");
        assert_eq!(r.apply(r"in C:\Users\mentor\x"), "in <path>");
    }

    #[test]
    fn values_after_secret_words_and_key_like_words_are_masked() {
        let r = redactor();
        assert_eq!(r.apply("token=abc123 next"), "token=[redacted] next");
        assert_eq!(
            r.apply("Authorization: Bearer xyz.789"),
            "Authorization: [redacted]"
        );
        assert_eq!(
            r.apply("using ghp_ABCDEF0123456789abcdef"),
            "using [redacted]"
        );
        assert_eq!(r.apply("id Zx9Qw2Er4Ty6Ui8Op0As1Df3Gh5"), "id [redacted]");
        assert_eq!(r.apply("tokens counted"), "tokens counted");
    }

    #[test]
    fn hashes_and_plain_words_stay() {
        let r = redactor();
        let sha = "a".repeat(64);
        assert_eq!(r.apply(&format!("sha256 {sha}")), format!("sha256 {sha}"));
        assert_eq!(
            r.apply("install-nv-rs finished in 12 ms"),
            "install-nv-rs finished in 12 ms"
        );
    }

    #[test]
    fn the_nexus_apikey_header_is_masked_in_every_form() {
        let r = redactor();
        let key = "Fake0Nexus1Key2Zz";
        for line in [
            format!("apikey: {key}"),
            format!("APIKEY: {key}"),
            format!("ApiKey:{key}"),
            format!("header = \"apikey: {key}\""),
            format!("-H 'apikey: {key}'"),
            format!("\"apikey\": \"{key}\""),
            format!("apikey={key}"),
        ] {
            let out = r.apply(&line);
            assert!(!out.contains(key), "{line} -> {out}");
            assert!(out.contains(KEY_MASK), "{line} -> {out}");
        }
        assert_eq!(
            r.apply(&format!("header = \"apikey: {key}\"")),
            "header = \"apikey: [redacted]\""
        );
    }

    #[test]
    fn an_apikey_query_pair_is_masked_and_the_rest_of_the_url_stays() {
        let r = redactor();
        assert_eq!(
            r.apply("GET https://api.example.test/v1/x?apikey=fake1234&page=2 ok"),
            "GET https://api.example.test/v1/x?apikey=[redacted]&page=2 ok"
        );
    }

    #[test]
    fn an_nxm_link_key_is_masked_in_either_query_position() {
        let r = redactor();
        assert_eq!(
            r.apply("nxm://game/mods/1/files/2?key=fake-key_9&expires=1700000000&user_id=3"),
            "nxm://game/mods/1/files/2?key=[redacted]&expires=1700000000&user_id=3"
        );
        assert_eq!(
            r.apply("nxm://game/mods/1/files/2?expires=1&KEY=fake9&user_id=3"),
            "nxm://game/mods/1/files/2?expires=1&KEY=[redacted]&user_id=3"
        );
        assert_eq!(r.apply("open /a?x=1&key= done"), "open <path> done");
        assert_eq!(r.apply("monkey=1 ?keyboard=2"), "monkey=1 ?keyboard=2");
    }

    #[test]
    fn nexus_api_key_environment_assignments_are_masked() {
        let r = redactor();
        assert_eq!(
            r.apply("NEXUS_API_KEY=fake123 NEXUS_API_KEY: fake456"),
            "NEXUS_API_KEY=[redacted] NEXUS_API_KEY: [redacted]"
        );
        assert_eq!(
            r.apply("export NEXUS_API_KEY=\"fake123\""),
            "export NEXUS_API_KEY=\"[redacted]\""
        );
    }
}
