// How a finished upload is announced: a toast in the window when it's in front, else a system
// notification; with a "Copy link" button unless the link was copied by itself (a setting).
// Uploads that finish one after another (bulk, or carried on after a restart) get one summary.

use std::sync::Mutex;

/// An upload that finished since the queue was last empty.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub title: String,
    pub link: String,
    pub replaced: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// In the window. `link` empty: no Copy button.
    Toast { title: String, link: String, copied: bool },
    /// A system notification; `link` Some: with a Copy link button.
    System { title: String, body: String, link: Option<String>, copied: bool },
    Nothing,
}

pub fn finish_notice(finished: &[Finished], still_queued: usize, focused: bool, auto_copy: bool) -> Notice {
    if still_queued > 0 {
        return Notice::Nothing;
    }
    let (title, body, link, copied) = match finished {
        [] => return Notice::Nothing,
        [one] if auto_copy => ("Link copied".to_string(), one.title.clone(), None, true),
        [one] => {
            let title = if one.replaced { "Video replaced" } else { "Clip uploaded" };
            (title.to_string(), one.title.clone(), Some(one.link.clone()), false)
        }
        many => (format!("{} clips uploaded", many.len()), "Their links are in My clips.".to_string(), None, false),
    };
    if focused {
        Notice::Toast { title, link: link.unwrap_or_default(), copied }
    } else {
        Notice::System { title, body, link, copied }
    }
}

/// The uploads finished since the queue was last empty.
#[derive(Default)]
pub struct UploadBatch(Mutex<Vec<Finished>>);

impl UploadBatch {
    pub fn push(&self, f: Finished) {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(f);
    }

    /// Everything finished so far, once nothing is left queued (`still_queued == 0`).
    pub fn take_if_idle(&self, still_queued: usize) -> Option<Vec<Finished>> {
        let mut list = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if still_queued > 0 || list.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut *list))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(title: &str, replaced: bool) -> Finished {
        Finished { title: title.into(), link: format!("https://k/c/{title}"), replaced }
    }

    #[test]
    fn one_upload_unfocused_gets_a_notification_with_copy() {
        assert_eq!(
            finish_notice(&[done("Ace", false)], 0, false, false),
            Notice::System { title: "Clip uploaded".into(), body: "Ace".into(), link: Some("https://k/c/Ace".into()), copied: false }
        );
    }

    #[test]
    fn one_upload_focused_gets_a_toast_with_copy() {
        assert_eq!(
            finish_notice(&[done("Ace", false)], 0, true, false),
            Notice::Toast { title: "Clip uploaded".into(), link: "https://k/c/Ace".into(), copied: false }
        );
    }

    #[test]
    fn auto_copy_drops_the_button() {
        assert_eq!(
            finish_notice(&[done("Ace", false)], 0, false, true),
            Notice::System { title: "Link copied".into(), body: "Ace".into(), link: None, copied: true }
        );
        assert_eq!(
            finish_notice(&[done("Ace", false)], 0, true, true),
            Notice::Toast { title: "Link copied".into(), link: String::new(), copied: true }
        );
    }

    #[test]
    fn replaced_says_video_replaced() {
        let Notice::System { title, .. } = finish_notice(&[done("Ace", true)], 0, false, false) else { panic!() };
        assert_eq!(title, "Video replaced");
    }

    #[test]
    fn nothing_while_more_are_queued() {
        assert_eq!(finish_notice(&[done("Ace", false)], 2, false, false), Notice::Nothing);
        assert_eq!(finish_notice(&[], 0, false, false), Notice::Nothing);
    }

    #[test]
    fn several_finished_get_one_summary() {
        let all = [done("A", false), done("B", false), done("C", true)];
        assert_eq!(
            finish_notice(&all, 0, false, false),
            Notice::System { title: "3 clips uploaded".into(), body: "Their links are in My clips.".into(), link: None, copied: false }
        );
        assert_eq!(
            finish_notice(&all, 0, true, false),
            Notice::Toast { title: "3 clips uploaded".into(), link: String::new(), copied: false }
        );
    }

    #[test]
    fn batch_is_emptied_once_idle() {
        let batch = UploadBatch::default();
        batch.push(done("A", false));
        batch.push(done("B", false));
        assert_eq!(batch.take_if_idle(1), None);
        assert_eq!(batch.take_if_idle(0).map(|f| f.len()), Some(2));
        assert_eq!(batch.take_if_idle(0), None);
    }
}
