//! Review comment commands: `comment.add`, `comment.reply`, `comment.edit`, `comment.delete`,
//! `comment.list`, and the walkers `comment.show`, `comment.next` and `comment.previous`. A
//! comment (`Document.comments`) is anchored to a page and optionally to a shape on it, has an
//! author and a time stamped here, and carries a flat list of replies.

use serde_json::{Value, json};
use vizcraft_diagram::{Comment, CommentAnchor, Reply};
use vizcraft_model::MAX_TEXT;

use super::link::show_page;
use super::{opt_str, select, short};
use crate::{CmdResult, CommandSpec, EngineError, Session};

/// Where the commands live in the UI.
const LOCATION: &str = "Review > Comments";

/// Return the comment commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("comment.add", "New Comment", LOCATION, add)
            .params(r#"{"text": string, "page"?: id|name|index (default: the active page), "shape"?: shape on that page (default: the selected shape; "" or null anchors to the page)} -> {id, page, shape?}; the author is the session author, the time is now"#),
        CommandSpec::new("comment.reply", "Reply to Comment", LOCATION, reply)
            .params(r#"{"id": comment, "text": string} -> {id, replies}"#)
            .when(has_comments),
        CommandSpec::new("comment.edit", "Edit Comment", LOCATION, edit)
            .params(r#"{"id": comment, "text": string, "reply"?: n (edit that reply instead)} -> {id}"#)
            .when(has_comments),
        CommandSpec::new("comment.delete", "Delete Comment", LOCATION, delete)
            .params(r#"{"id"?: comment (default: every comment on the selected shapes), "reply"?: n (delete only that reply)} -> {deleted: [ids]}"#)
            .when(has_comments),
        CommandSpec::new("comment.list", "List Comments", LOCATION, list)
            .params(r#"{"page"?: id|name|index (default: every page)} -> {comments: [{id, page, pageName, shape?, author, time, text, replies: [{author, time, text}]}]}"#),
        CommandSpec::new("comment.show", "Go to Comment", LOCATION, show)
            .params(r#"{"id": comment} -> {id, page, shape?}; shows its page and selects its shape"#)
            .when(has_comments),
        CommandSpec::new("comment.next", "Next Comment", LOCATION, |s, _| walk(s, 1))
            .params("{} -> {id, page, shape?}; the comment after the one on the selected shape, wrapping round")
            .when(has_comments),
        CommandSpec::new("comment.previous", "Previous Comment", LOCATION, |s, _| walk(s, -1))
            .params("{} -> {id, page, shape?}; the comment before the one on the selected shape, wrapping round")
            .when(has_comments),
    ]
}

fn has_comments(s: &Session) -> Option<&'static str> {
    if s.doc.comments.is_empty() { Some("add a comment first") } else { None }
}

/// A page param as text: a string or a number meaning an index.
fn page_ref(v: &Value) -> Result<Option<String>, EngineError> {
    match v.get("page") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => Ok(Some(t.clone())),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(EngineError::Params("`page` must be a page id, name or index".into())),
    }
}

/// The index of the page the `page` param names, else the active page.
fn page_index(s: &Session, v: &Value) -> Result<usize, EngineError> {
    match page_ref(v)? {
        Some(r) => s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r)))),
        None => Ok(s.doc.active_index()),
    }
}

fn text_param(v: &Value) -> Result<String, EngineError> {
    let t = opt_str(v, "text")?
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| EngineError::Params("`text` (non-empty string) is required".into()))?;
    if t.len() > MAX_TEXT {
        return Err(EngineError::Params(format!("`text` is longer than {MAX_TEXT} bytes")));
    }
    Ok(t.to_string())
}

fn position(s: &Session, v: &Value) -> Result<usize, EngineError> {
    let id = opt_str(v, "id")?.ok_or_else(|| EngineError::Params("`id` (comment) is required".into()))?;
    s.doc.comments.iter().position(|c| c.id == id).ok_or_else(|| EngineError::Params(format!("no comment `{}`", short(id))))
}

fn reply_index(v: &Value) -> Result<Option<usize>, EngineError> {
    match v.get("reply") {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| EngineError::Params("`reply` must be a whole number".into())),
    }
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let text = text_param(v)?;
    let pi = page_index(s, v)?;
    if s.doc.comments.len() >= Comment::MAX_COMMENTS {
        return Err(EngineError::Params(format!("a document has at most {} comments", Comment::MAX_COMMENTS)));
    }
    let page = s.doc.pages.get(pi).ok_or_else(|| EngineError::Failed("the document has no page".into()))?;
    let shape = match v.get("shape") {
        Some(Value::Null) => None,
        Some(Value::String(t)) if t.is_empty() => None,
        Some(Value::String(t)) => Some(t.clone()),
        Some(_) => return Err(EngineError::Params("`shape` must be a shape id".into())),
        // Without a `shape` the comment goes on the selected shape, when the page is the active one.
        None if pi == s.doc.active_index() => s.sel.primary.clone().filter(|i| page.shape(i).is_some()),
        None => None,
    };
    if let Some(sh) = &shape
        && page.shape(sh).is_none()
    {
        return Err(EngineError::Params(format!("`{}` is not a shape on that page", short(sh))));
    }
    let page_id = page.id.clone();
    let id = s.doc.next_id("cm");
    s.doc.comments.push(Comment {
        id: id.clone(),
        anchor: CommentAnchor { page: page_id.clone(), shape: shape.clone() },
        author: s.author.clone(),
        time: super::now_iso(),
        text,
        replies: Vec::new(),
    });
    Ok(json!({"id": id, "page": page_id, "shape": shape}))
}

fn reply(s: &mut Session, v: &Value) -> CmdResult {
    let text = text_param(v)?;
    let i = position(s, v)?;
    let (author, time) = (s.author.clone(), super::now_iso());
    let c = s.doc.comments.get_mut(i).ok_or_else(|| EngineError::Failed("the comment vanished".into()))?;
    if c.replies.len() >= Comment::MAX_REPLIES {
        return Err(EngineError::Params(format!("a comment has at most {} replies", Comment::MAX_REPLIES)));
    }
    c.replies.push(Reply { author, time, text });
    Ok(json!({"id": c.id, "replies": c.replies.len()}))
}

fn edit(s: &mut Session, v: &Value) -> CmdResult {
    let text = text_param(v)?;
    let i = position(s, v)?;
    let reply = reply_index(v)?;
    let c = s.doc.comments.get_mut(i).ok_or_else(|| EngineError::Failed("the comment vanished".into()))?;
    match reply {
        Some(r) => c.replies.get_mut(r).ok_or_else(|| EngineError::Params(format!("the comment has no reply {r}")))?.text = text,
        None => c.text = text,
    }
    Ok(json!({"id": c.id}))
}

fn delete(s: &mut Session, v: &Value) -> CmdResult {
    if opt_str(v, "id")?.is_some() {
        let i = position(s, v)?;
        let reply = reply_index(v)?;
        let c = s.doc.comments.get_mut(i).ok_or_else(|| EngineError::Failed("the comment vanished".into()))?;
        if let Some(r) = reply {
            if r >= c.replies.len() {
                return Err(EngineError::Params(format!("the comment has no reply {r}")));
            }
            c.replies.remove(r);
            return Ok(json!({"deleted": []}));
        }
        let gone = s.doc.comments.remove(i);
        return Ok(json!({"deleted": [gone.id]}));
    }
    // No id: the comments pinned to the selected shapes of the active page.
    let Some(page) = s.doc.page() else { return Err(EngineError::Failed("the document has no page".into())) };
    let (pid, sel) = (page.id.clone(), s.sel.items.clone());
    let on_selection = |c: &Comment| c.anchor.page == pid && c.anchor.shape.as_ref().is_some_and(|sh| sel.contains(sh));
    let deleted: Vec<String> = s.doc.comments.iter().filter(|c| on_selection(c)).map(|c| c.id.clone()).collect();
    if deleted.is_empty() {
        return Err(EngineError::Params("pass `id`, or select a shape that has a comment".into()));
    }
    s.doc.comments.retain(|c| !deleted.contains(&c.id));
    Ok(json!({"deleted": deleted}))
}

/// One comment as the list shows it.
fn comment_json(s: &Session, c: &Comment) -> Value {
    let name = s.doc.pages.iter().find(|p| p.id == c.anchor.page).map(|p| p.name.clone());
    let replies: Vec<Value> = c.replies.iter().map(|r| json!({"author": r.author, "time": r.time, "text": r.text})).collect();
    json!({"id": c.id, "page": c.anchor.page, "pageName": name, "shape": c.anchor.shape, "author": c.author, "time": c.time, "text": c.text, "replies": replies})
}

fn list(s: &mut Session, v: &Value) -> CmdResult {
    let only = match page_ref(v)? {
        Some(r) => {
            let i = s.doc.find_page(&r).ok_or_else(|| EngineError::Params(format!("no page `{}`", short(&r))))?;
            s.doc.pages.get(i).map(|p| p.id.clone())
        }
        None => None,
    };
    let comments: Vec<Value> = s.doc.comments.iter().filter(|c| only.as_ref().is_none_or(|p| *p == c.anchor.page)).map(|c| comment_json(s, c)).collect();
    Ok(json!({"comments": comments}))
}

/// Go to comment `i`: show its page and select its shape (nothing for a page comment).
fn go_to(s: &mut Session, i: usize) -> CmdResult {
    let c = s.doc.comments.get(i).cloned().ok_or_else(|| EngineError::Failed("the comment vanished".into()))?;
    let pi = s.doc.pages.iter().position(|p| p.id == c.anchor.page).ok_or_else(|| EngineError::Params("the comment's page no longer exists".into()))?;
    show_page(s, pi);
    let shape = c.anchor.shape.filter(|sh| s.doc.page().is_some_and(|p| p.shape(sh).is_some()));
    select(s, shape.iter().cloned().collect());
    Ok(json!({"id": c.id, "page": c.anchor.page, "shape": shape}))
}

fn show(s: &mut Session, v: &Value) -> CmdResult {
    let i = position(s, v)?;
    go_to(s, i)
}

/// Step to the next (`+1`) or previous (`-1`) comment in document order, counting from the
/// comment on the selected shape; with none selected, next is the first and previous the last.
fn walk(s: &mut Session, step: i64) -> CmdResult {
    let len = s.doc.comments.len();
    let here = s.doc.page().and_then(|page| {
        let sel = s.sel.primary.as_ref()?;
        s.doc.comments.iter().position(|c| c.anchor.page == page.id && c.anchor.shape.as_ref() == Some(sel))
    });
    let to = match (here, step > 0) {
        (Some(i), true) => i.saturating_add(1) % len.max(1),
        (Some(i), false) => i.checked_sub(1).unwrap_or(len.saturating_sub(1)),
        (None, true) => 0,
        (None, false) => len.saturating_sub(1),
    };
    go_to(s, to)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{Session, sample};

    fn session() -> Session {
        Session::new(sample::sample_document())
    }

    fn id_of(s: &Session, text: &str) -> String {
        s.doc.page().and_then(|p| p.shapes.iter().find(|x| x.text == text)).map(|x| x.id.clone()).unwrap()
    }

    #[test]
    fn a_thread_round_trips_and_lists_per_page() {
        let mut s = session();
        s.author = "Ada".into();
        let a = id_of(&s, "Start");
        let c = s.run("comment.add", &json!({"text": "Why here?", "shape": a})).unwrap()["id"].as_str().unwrap().to_string();
        s.run("comment.reply", &json!({"id": c, "text": "Because."})).unwrap();
        s.run("comment.edit", &json!({"id": c, "text": "Why start here?"})).unwrap();
        s.run("comment.edit", &json!({"id": c, "reply": 0, "text": "Because it is first."})).unwrap();
        s.run("page.add", &json!({"name": "Two"})).unwrap();
        s.run("comment.add", &json!({"text": "On the second page"})).unwrap();
        let doc: vizcraft_diagram::Document = serde_json::from_value(serde_json::to_value(&s.doc).unwrap()).unwrap();
        assert_eq!(doc.comments, s.doc.comments);
        assert_eq!(doc.comments[0].author, "Ada");
        assert!(doc.comments[0].time.ends_with('Z'));
        let all = s.run("comment.list", &json!({})).unwrap();
        assert_eq!(all["comments"].as_array().unwrap().len(), 2);
        let two = s.run("comment.list", &json!({"page": "Two"})).unwrap();
        assert_eq!(two["comments"].as_array().unwrap().len(), 1);
        assert_eq!(two["comments"][0]["text"], "On the second page");
        let one = s.run("comment.list", &json!({"page": 0})).unwrap();
        assert_eq!(one["comments"][0]["replies"][0]["text"], "Because it is first.");
        s.run("comment.delete", &json!({"id": c, "reply": 0})).unwrap();
        assert!(s.doc.comments[0].replies.is_empty());
        s.run("comment.delete", &json!({"id": c})).unwrap();
        assert_eq!(s.doc.comments.len(), 1);
    }

    #[test]
    fn bad_comments_are_refused() {
        let mut s = session();
        assert!(s.run("comment.add", &json!({"text": "  "})).is_err());
        assert!(s.run("comment.add", &json!({"text": "x", "shape": "zz9"})).is_err());
        assert!(s.run("comment.add", &json!({"text": "x", "page": "nowhere"})).is_err());
        assert!(s.run("comment.reply", &json!({"id": "cm1", "text": "x"})).is_err(), "disabled with no comments");
        assert!(s.doc.comments.is_empty());
    }

    #[test]
    fn next_and_previous_select_the_anchor_and_wrap() {
        let mut s = session();
        let (a, b) = (id_of(&s, "Start"), id_of(&s, "End"));
        s.run("comment.add", &json!({"text": "one", "shape": a})).unwrap();
        s.run("comment.add", &json!({"text": "two", "shape": b})).unwrap();
        s.run("edit.selectNone", &json!({})).unwrap();
        s.run("comment.next", &json!({})).unwrap();
        assert_eq!(s.sel.items, vec![a.clone()]);
        s.run("comment.next", &json!({})).unwrap();
        assert_eq!(s.sel.items, vec![b.clone()]);
        s.run("comment.next", &json!({})).unwrap();
        assert_eq!(s.sel.items, vec![a.clone()]);
        s.run("comment.previous", &json!({})).unwrap();
        assert_eq!(s.sel.items, vec![b.clone()]);
        s.run("edit.select", &json!({"ids": [a.clone()]})).unwrap();
        s.run("comment.delete", &json!({})).unwrap();
        assert_eq!(s.doc.comments.len(), 1);
    }
}
