//! Document-wide data beyond pages: file properties, review comments, the image store and
//! document protection. All plain data.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::MAX_TEXT;
use crate::model::{MAX_ID, truncate};
use crate::serde_util::{base64, lenient, lenient_vec};

/// File properties as a properties dialog shows them. `title` here is the stored property; the
/// document's own `title` is what the title bar shows before the file has a name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DocProperties {
    /// Title property.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Author property.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Subject property.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub subject: String,
    /// Keywords, free text (usually separated by commas or semicolons).
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub keywords: String,
    /// Longer description.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Named document properties that packs declare (`legalOwner`, `identificationNumber`, the
    /// ISO 7200 and ASME title block fields): plain keys, at most [`DocProperties::MAX_FIELDS`],
    /// values capped at [`DocProperties::MAX_SHORT`]. Title blocks show them through
    /// `{{doc:key}}` fields.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

impl DocProperties {
    /// Longest title, author, subject or keywords kept, in bytes; the description takes
    /// [`MAX_TEXT`].
    pub const MAX_SHORT: usize = 1024;
    /// Most named properties kept.
    pub const MAX_FIELDS: usize = 64;
    /// Longest named property key kept.
    pub const MAX_KEY: usize = 64;

    /// Whether `key` may name a property in `fields`: letters, digits and `_`, at most
    /// [`DocProperties::MAX_KEY`] bytes.
    pub fn is_field_key(key: &str) -> bool {
        !key.is_empty() && key.len() <= Self::MAX_KEY && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// Cap every property's length, and the named properties' count and keys.
    pub fn sanitize(&mut self) {
        for s in [&mut self.title, &mut self.author, &mut self.subject, &mut self.keywords] {
            truncate(s, Self::MAX_SHORT);
        }
        truncate(&mut self.description, MAX_TEXT);
        self.fields.retain(|k, _| Self::is_field_key(k));
        while self.fields.len() > Self::MAX_FIELDS {
            self.fields.pop_last();
        }
        for v in self.fields.values_mut() {
            truncate(v, Self::MAX_SHORT);
        }
    }
}

/// What a comment is attached to: a page, and optionally one shape on it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CommentAnchor {
    /// The page id.
    #[serde(deserialize_with = "lenient")]
    pub page: String,
    /// The shape id on that page; `None` anchors the comment to the page itself.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
}

/// One reply in a comment thread.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Reply {
    /// Who wrote it.
    #[serde(deserialize_with = "lenient")]
    pub author: String,
    /// ISO-8601 timestamp.
    #[serde(deserialize_with = "lenient")]
    pub time: String,
    /// The reply text.
    #[serde(deserialize_with = "lenient")]
    pub text: String,
}

/// A review comment with its replies.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Comment {
    /// Unique id among the comments.
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Where it is attached.
    #[serde(deserialize_with = "lenient")]
    pub anchor: CommentAnchor,
    /// Who wrote it.
    #[serde(deserialize_with = "lenient")]
    pub author: String,
    /// ISO-8601 timestamp.
    #[serde(deserialize_with = "lenient")]
    pub time: String,
    /// The comment text.
    #[serde(deserialize_with = "lenient")]
    pub text: String,
    /// Replies, oldest first, at most [`Comment::MAX_REPLIES`].
    #[serde(deserialize_with = "lenient_vec", skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<Reply>,
}

/// Longest author name kept, in bytes.
const MAX_AUTHOR: usize = 256;
/// Longest timestamp kept, in bytes.
const MAX_TIME: usize = 64;

impl Comment {
    /// Most comments one document keeps.
    pub const MAX_COMMENTS: usize = 10_000;
    /// Most replies one comment keeps.
    pub const MAX_REPLIES: usize = 1000;

    /// Cap ids, names, timestamps, text and the reply count.
    pub fn sanitize(&mut self) {
        truncate(&mut self.id, MAX_ID);
        truncate(&mut self.anchor.page, MAX_ID);
        if let Some(s) = &mut self.anchor.shape {
            truncate(s, MAX_ID);
        }
        truncate(&mut self.author, MAX_AUTHOR);
        truncate(&mut self.time, MAX_TIME);
        truncate(&mut self.text, MAX_TEXT);
        self.replies.truncate(Self::MAX_REPLIES);
        for r in &mut self.replies {
            truncate(&mut r.author, MAX_AUTHOR);
            truncate(&mut r.time, MAX_TIME);
            truncate(&mut r.text, MAX_TEXT);
        }
    }
}

/// A picture in the document's image store; shapes show it through an
/// [`ImageRef`](crate::shape_fields::ImageRef). The bytes are base64 on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ImageAsset {
    /// Unique id within the store.
    #[serde(deserialize_with = "lenient")]
    pub id: String,
    /// Media type such as `image/png`; anything not `image/...` is dropped on load.
    #[serde(deserialize_with = "lenient")]
    pub media_type: String,
    /// The encoded picture, at most [`ImageAsset::MAX_BYTES`].
    #[serde(with = "base64")]
    pub bytes: Vec<u8>,
}

impl ImageAsset {
    /// Largest single picture kept, in bytes.
    pub const MAX_BYTES: usize = 16 * 1024 * 1024;
    /// Largest total of all pictures kept, in bytes.
    pub const MAX_STORE_BYTES: usize = 48 * 1024 * 1024;
    /// Most pictures one document keeps.
    pub const MAX_ASSETS: usize = 1024;
    /// Longest media type kept, in bytes.
    pub const MAX_MEDIA_TYPE: usize = 128;

    /// Whether the media type is a plain `image/<subtype>` name.
    pub fn has_image_type(&self) -> bool {
        self.media_type.len() <= Self::MAX_MEDIA_TYPE
            && self
                .media_type
                .strip_prefix("image/")
                .is_some_and(|sub| !sub.is_empty() && sub.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-')))
    }
}

/// Keep the store bounded: assets with an image media type, a plain unique id and from 1 to
/// [`ImageAsset::MAX_BYTES`] bytes, at most [`ImageAsset::MAX_ASSETS`] of them and at most
/// [`ImageAsset::MAX_STORE_BYTES`] in all (later assets past the total are dropped).
pub(crate) fn sanitize_images(images: &mut Vec<ImageAsset>) {
    let mut seen = HashSet::new();
    let mut total: usize = 0;
    images.retain(|a| {
        let ok = crate::is_plain_id(&a.id)
            && a.has_image_type()
            && !a.bytes.is_empty()
            && a.bytes.len() <= ImageAsset::MAX_BYTES
            && total.saturating_add(a.bytes.len()) <= ImageAsset::MAX_STORE_BYTES
            && seen.insert(a.id.clone());
        if ok {
            total = total.saturating_add(a.bytes.len());
        }
        ok
    });
    images.truncate(ImageAsset::MAX_ASSETS);
}

/// Cap the comment count and every comment's fields.
pub(crate) fn sanitize_comments(comments: &mut Vec<Comment>) {
    comments.truncate(Comment::MAX_COMMENTS);
    comments.iter_mut().for_each(Comment::sanitize);
}

/// Document protection, as `doc.protect` sets it. All off by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Protection {
    /// Every shape refuses to move, resize, rotate, be deleted or have its text changed, as if
    /// all its locks were set.
    #[serde(deserialize_with = "lenient")]
    pub shapes: bool,
    /// Background pages refuse changes to their shapes, and no page's background can be changed.
    #[serde(deserialize_with = "lenient")]
    pub backgrounds: bool,
}

/// Longest language tag kept, in bytes (the BCP 47 limit for practical tags).
pub const MAX_LANGUAGE: usize = 35;
/// The language tag that turns spelling checks off for a shape.
pub const LANGUAGE_NONE: &str = "none";
/// Most ignored words one document keeps.
pub const MAX_IGNORED_WORDS: usize = 5000;
/// Longest ignored word kept, in bytes.
pub const MAX_IGNORED_WORD: usize = 64;

/// A BCP 47 tag in its usual case (`en-us` becomes `en-US`, `zh-hant` becomes `zh-Hant`), or
/// [`LANGUAGE_NONE`]. `None` when the text is not a plain tag: empty, too long, or with a
/// segment that is empty, longer than eight characters or not ASCII letters and digits.
pub fn normalize_language(tag: &str) -> Option<String> {
    let tag = tag.trim().replace('_', "-");
    if tag.eq_ignore_ascii_case(LANGUAGE_NONE) {
        return Some(LANGUAGE_NONE.to_string());
    }
    if tag.is_empty() || tag.len() > MAX_LANGUAGE {
        return None;
    }
    let mut out = Vec::new();
    for (i, seg) in tag.split('-').enumerate() {
        if seg.is_empty() || seg.len() > 8 || !seg.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        if i == 0 && (!(2..=3).contains(&seg.len()) || !seg.chars().all(|c| c.is_ascii_alphabetic())) {
            return None;
        }
        let alpha = seg.chars().all(|c| c.is_ascii_alphabetic());
        out.push(match (i, seg.len(), alpha) {
            (0, ..) => seg.to_ascii_lowercase(),
            (_, 4, true) => {
                let lower = seg.to_ascii_lowercase();
                let mut cs = lower.chars();
                cs.next().map(|f| f.to_ascii_uppercase().to_string() + cs.as_str()).unwrap_or_default()
            }
            (_, 2, true) => seg.to_ascii_uppercase(),
            _ => seg.to_ascii_lowercase(),
        });
    }
    Some(out.join("-"))
}

/// Replace a language tag by its normal form, or clear it when it is not a tag.
pub(crate) fn sanitize_language(tag: &mut String) {
    *tag = normalize_language(tag).unwrap_or_default();
}

/// Keep the ignored words bounded, lower case and unique; words with no letter are dropped.
pub(crate) fn sanitize_ignored_words(words: &mut Vec<String>) {
    let mut seen = HashSet::new();
    for w in words.iter_mut() {
        *w = w.trim().to_lowercase();
        truncate(w, MAX_IGNORED_WORD);
    }
    words.retain(|w| w.chars().any(char::is_alphabetic) && seen.insert(w.clone()));
    words.truncate(MAX_IGNORED_WORDS);
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn tags_take_their_usual_case_and_junk_is_refused() {
        assert_eq!(normalize_language("en-us").as_deref(), Some("en-US"));
        assert_eq!(normalize_language("zh_hant_tw").as_deref(), Some("zh-Hant-TW"));
        assert_eq!(normalize_language(" NONE ").as_deref(), Some("none"));
        for bad in ["", "e", "en--US", "en-US-", "1n", "en us", "toolonglanguage", "en-\u{e9}"] {
            assert_eq!(normalize_language(bad), None, "{bad}");
        }
    }

    #[test]
    fn ignored_words_are_lowered_deduplicated_and_capped() {
        let mut w: Vec<String> = ["Teh", "teh", "  ", "123", "x".repeat(200).as_str()].iter().map(|s| s.to_string()).collect();
        sanitize_ignored_words(&mut w);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], "teh");
        assert_eq!(w[1].len(), MAX_IGNORED_WORD);
    }
}
