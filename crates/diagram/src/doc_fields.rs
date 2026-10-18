//! Document-wide data beyond pages: file properties, review comments and the image store. All
//! plain data; no command reads them yet.

use std::collections::HashSet;

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
}

impl DocProperties {
    /// Longest title, author, subject or keywords kept, in bytes; the description takes
    /// [`MAX_TEXT`].
    pub const MAX_SHORT: usize = 1024;

    /// Cap every property's length.
    pub fn sanitize(&mut self) {
        for s in [&mut self.title, &mut self.author, &mut self.subject, &mut self.keywords] {
            truncate(s, Self::MAX_SHORT);
        }
        truncate(&mut self.description, MAX_TEXT);
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
