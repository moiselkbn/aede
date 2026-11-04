//! Cleans up the text given by the LLM and refuses the answers that cannot be trusted.
//!
//! Most dictations are requests to an AI ("écris un mail à..."): the risk is a model that does
//! the task instead of rewriting the request, or that summarizes it. Both show in the length.

/// Below this length (in characters) the ratio is too noisy to mean anything.
const MIN_CHECKED_LENGTH: usize = 20;
/// Measured at prototype 3: no good answer was outside these limits (docs/prototype-3.md).
const MAX_RATIO: f32 = 1.5;
const MIN_RATIO: f32 = 0.5;

/// Straight apostrophes only (some models write curly ones), no spaces around the text.
pub fn normalize(text: &str) -> String {
    text.replace('\u{2019}', "'").trim().to_string()
}

/// `Ok` if the rewritten text looks like a rewrite of `input`, otherwise a message for the user.
pub fn verify(input: &str, output: &str) -> Result<(), String> {
    if output.is_empty() {
        return Err("la reformulation est vide".to_string());
    }
    let input_length = input.chars().count();
    if input_length < MIN_CHECKED_LENGTH {
        return Ok(());
    }
    let ratio = output.chars().count() as f32 / input_length as f32;
    if ratio > MAX_RATIO {
        return Err(
            "la reformulation est beaucoup plus longue que le texte dicté (la demande a peut-être été exécutée)"
                .to_string(),
        );
    }
    if ratio < MIN_RATIO {
        return Err(
            "la reformulation est beaucoup plus courte que le texte dicté (résumé ?)".to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curly_apostrophes_become_straight_ones() {
        assert_eq!(normalize("  J’ai l’idée \n"), "J'ai l'idée");
    }

    #[test]
    fn an_empty_answer_is_refused() {
        assert!(verify("Fais un merge sur main", "").is_err());
    }

    #[test]
    fn a_normal_rewrite_passes() {
        let input = "Euh j'aimerais que tu fasses un pull sur le projet";
        assert!(verify(input, "Fais un pull sur le projet").is_ok());
    }

    #[test]
    fn a_much_longer_answer_is_refused() {
        let input = "Écris un mail à monsieur Durand pour confirmer notre rendez-vous";
        let output = input.repeat(2);
        assert!(verify(input, &output).unwrap_err().contains("plus longue"));
    }

    #[test]
    fn a_much_shorter_answer_is_refused() {
        let input = "Écris un mail à monsieur Durand pour confirmer notre rendez-vous";
        assert!(
            verify(input, "Écris un mail")
                .unwrap_err()
                .contains("plus courte")
        );
    }

    #[test]
    fn very_short_texts_are_not_checked_by_length() {
        assert!(verify("Merci beaucoup", "Merci").is_ok());
    }
}
