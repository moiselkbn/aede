//! The prompt file (rules and examples) and the chat format of the model (ChatML, used by Qwen).
//!
//! The prompt file is a list of messages, each one introduced by a `[system]`, `[user]` or
//! `[assistant]` line. The system message holds the rules, the user/assistant pairs after it
//! are the examples shown to the model. The dictated text is added as the last user message.

pub struct Message {
    role: String,
    content: String,
}

pub fn parse(text: &str) -> Result<Vec<Message>, String> {
    let mut messages: Vec<Message> = Vec::new();
    for line in text.lines() {
        let tag = line.trim();
        if matches!(tag, "[system]" | "[user]" | "[assistant]") {
            messages.push(Message {
                role: tag[1..tag.len() - 1].to_string(),
                content: String::new(),
            });
        } else if let Some(last) = messages.last_mut() {
            if !last.content.is_empty() {
                last.content.push('\n');
            }
            last.content.push_str(line);
        }
    }
    for message in &mut messages {
        message.content = message.content.trim().to_string();
        if message.content.is_empty() {
            return Err(format!(
                "Le prompt de reformulation a un message [{}] vide.",
                message.role
            ));
        }
    }
    if messages.first().map(|m| m.role.as_str()) != Some("system") {
        return Err("Le prompt de reformulation doit commencer par [system].".to_string());
    }
    Ok(messages)
}

fn turn(role: &str, content: &str) -> String {
    format!("<|im_start|>{role}\n{content}<|im_end|>\n")
}

/// The start of every prompt: the rules and the examples. It is the same for every dictation.
pub fn prefix(messages: &[Message]) -> String {
    messages
        .iter()
        .map(|message| turn(&message.role, &message.content))
        .collect()
}

/// The full text sent to the model: the prefix, then `input` as the last user message, then
/// the opening of the assistant answer.
pub fn render(messages: &[Message], input: &str) -> String {
    format!(
        "{}{}<|im_start|>assistant\n",
        prefix(messages),
        turn("user", input)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_messages_and_renders_chatml() {
        let messages = parse("[system]\nRegles\n\n[user]\nA\n[assistant]\nB\n").unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(
            render(&messages, "C"),
            "<|im_start|>system\nRegles<|im_end|>\n<|im_start|>user\nA<|im_end|>\n<|im_start|>assistant\nB<|im_end|>\n<|im_start|>user\nC<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn the_prefix_ends_with_a_special_token_and_starts_every_prompt() {
        let messages = parse("[system]\nRegles\n[user]\nA\n[assistant]\nB\n").unwrap();
        assert!(prefix(&messages).ends_with("<|im_end|>\n"));
        assert!(render(&messages, "texte").starts_with(&prefix(&messages)));
    }

    #[test]
    fn rejects_a_prompt_without_system_message() {
        assert!(parse("[user]\nA\n").is_err());
    }

    #[test]
    fn rejects_an_empty_message() {
        assert!(parse("[system]\nRegles\n[user]\n[assistant]\nB\n").is_err());
    }

    #[test]
    fn the_prompt_of_the_app_is_valid() {
        let messages = parse(crate::rewrite::PROMPT).unwrap();
        assert_eq!(messages[0].role, "system");
        // The rules, then pairs of example (user, assistant).
        assert!(messages.len() >= 3 && messages.len() % 2 == 1);
    }
}
