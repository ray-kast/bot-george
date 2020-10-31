use serenity::utils::MessageBuilder;
use std::fmt::Display;

/// Because MessageBuilder is missing some edge cases
pub trait MessageBuilderExt {
    /// Catch empty mono blocks
    fn push_mono_safer<D: Display>(&mut self, content: D) -> &mut Self;
    /// Line variant of `push_mono_safer`
    fn push_mono_line_safer<D: Display>(&mut self, content: D) -> &mut Self;
}

impl MessageBuilderExt for MessageBuilder {
    fn push_mono_safer<D: Display>(&mut self, content: D) -> &mut Self {
        let content = content.to_string();

        if content.len() == 0 {
            self.push_mono_safe('\u{200c}')
        } else {
            self.push_mono_safe(content)
        }
    }

    fn push_mono_line_safer<D: Display>(&mut self, content: D) -> &mut Self {
        let content = content.to_string();

        if content.len() == 0 {
            self.push_mono_safe('\u{200c}')
        } else {
            self.push_mono_safe(content)
        }
    }
}
