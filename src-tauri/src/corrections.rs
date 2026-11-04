//! Finds what the user corrected in a dictation: compares the dictation with the text now in the
//! field, and returns the replaced parts as pairs (what was dictated, what is there now). Pure
//! text work, no access to the screen or the disk.
//!
//! Two ways to find the corrected parts. When the few words that were around the dictation at
//! the time of the paste are known (`Surroundings`), the exact place of the dictation in the
//! field is known too, and every replaced part in it is found. Otherwise the unchanged words
//! around one replaced part locate it, which gives a single pair.

/// What was dictated, and what the user wrote instead.
pub type Correction = (String, String);

/// A few words that were just before and just after the pasted dictation. Nothing before
/// means the dictation was at the start of the field, nothing after that it was at the end.
#[derive(Clone, Debug, PartialEq)]
pub struct Surroundings {
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// Words kept on each side of the pasted dictation.
const SURROUNDING_WORDS: usize = 3;

/// Most corrections accepted at once.
pub const MAX_PAIRS: usize = 3;

/// The words around `said` in `field`, read right after the paste.
pub fn surroundings_of(said: &str, field: &str) -> Result<Surroundings, String> {
    let (said, _) = words_with_lines(said);
    let (words, _) = words_with_lines(field);
    if said.is_empty() {
        return Err("dictée vide".into());
    }
    // The last match: the dictation just pasted is the most recent, usually the lowest one.
    let start = (0..=words.len().saturating_sub(said.len()))
        .rev()
        .find(|&i| words.len() >= said.len() && same_ci(&said, &words[i..i + said.len()]))
        .ok_or("texte collé introuvable dans le champ")?;
    let end = start + said.len();
    let owned = |slice: &[&str]| slice.iter().map(|w| w.to_string()).collect();
    Ok(Surroundings {
        before: owned(&words[start.saturating_sub(SURROUNDING_WORDS)..start]),
        after: owned(&words[end..(end + SURROUNDING_WORDS).min(words.len())]),
    })
}

/// The corrections made to the dictation `said`, found in the `field` text. With the
/// surroundings of the paste it looks in the exact place where the dictation was pasted, and
/// falls back to the unchanged words around a single part if that place is lost.
///
/// `selection` is what the user has selected in the field, if anything. When it is part of the
/// words that replaced the dictation, it says which of them are the correction: the other new
/// words are left out ("prunes" became "poires aussi", "poires" selected: prunes -> poires).
pub fn find_corrections(
    said: &str,
    field: &str,
    around: Option<&Surroundings>,
    selection: Option<&str>,
) -> Result<Vec<Correction>, String> {
    let selection: Vec<&str> = selection
        .map(|text| text.split_whitespace().collect())
        .unwrap_or_default();
    if let Some(around) = around
        && let Some(result) = exact_pairs(said, field, around, &selection)
    {
        return result;
    }
    find_replacement_in(said, field, &selection).map(|pair| vec![pair])
}

/// The selected words, if they are part of `new` and fewer than it: the user said which of the
/// new words are the correction. Otherwise `new` as it is.
fn narrow<'a, 'b>(new: &'b [&'a str], selection: &[&str]) -> &'b [&'a str] {
    if selection.is_empty() || selection.len() >= new.len() {
        return new;
    }
    match new
        .windows(selection.len())
        .position(|words| same_ci(selection, words))
    {
        Some(at) => &new[at..at + selection.len()],
        None => new,
    }
}

/// Same words, ignoring case and the punctuation around them.
fn same_ci(a: &[&str], b: &[&str]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            core(x).to_lowercase() == core(y).to_lowercase()
                || glued(&x.to_lowercase(), &y.to_lowercase())
        })
}

/// Where the dictation is in the field now: the words between the remembered ones before and
/// after it. Returns the first and last (excluded) word index, or None if the remembered words
/// are no longer there (the user changed them too). With nothing remembered on a side, the
/// dictation was at the start (or the end) of the field.
fn find_region(
    field: &[&str],
    before: &[String],
    after: &[String],
    said_len: usize,
) -> Option<(usize, usize)> {
    let m = field.len();
    let matches = |at: usize, words: &[String]| {
        let refs: Vec<&str> = words.iter().map(String::as_str).collect();
        at + refs.len() <= m && same_ci(&refs, &field[at..at + refs.len()])
    };
    let starts: Vec<usize> = if before.is_empty() {
        vec![0]
    } else {
        (before.len()..=m)
            .filter(|&p| matches(p - before.len(), before))
            .collect()
    };
    let ends: Vec<usize> = if after.is_empty() {
        vec![m]
    } else {
        (0..m).filter(|&q| matches(q, after)).collect()
    };
    let mut best: Option<(usize, usize)> = None;
    for &p in &starts {
        for &q in ends
            .iter()
            .filter(|&&q| q > p && q - p <= said_len + MAX_SPAN)
        {
            let better = best
                .is_none_or(|(bp, bq)| (q - p).abs_diff(said_len) < (bq - bp).abs_diff(said_len));
            if better {
                best = Some((p, q));
            }
        }
    }
    best
}

/// One replaced part found by the comparison.
struct Hunk<'a> {
    /// The words dictated.
    old: Vec<&'a str>,
    /// The words now in their place.
    new: Vec<&'a str>,
    /// Where it starts and where it ends, as (word of the dictation, word of the region).
    begin: (usize, usize),
    end: (usize, usize),
}

/// What the user is told when a correction at the edge of the dictation may include words
/// added next to it.
fn ambiguity_message(old: &[&str], new: &[&str]) -> String {
    format!(
        "Je ne sais pas si « {} » est tout entier la correction de « {} » ou s'il contient des \
         mots ajoutés après. Garde le point final de la dictée, ou mets le texte voisin à la \
         ligne, puis refais le raccourci.",
        phrase(new),
        phrase(old)
    )
}

/// Compares the dictation with the words now in its place, word by word, and returns the
/// replaced parts (what was dictated, what is there now). More than `MAX_PAIRS` corrections are
/// refused.
///
/// `open_start` and `open_end` say that nothing delimits the region on that side (the dictation
/// was at the start or at the end of the field). A replaced part that touches such a side and
/// became longer may include words the user added next to it: it is refused, not guessed.
fn diff_pairs(
    said: &[&str],
    region: &[&str],
    open_start: bool,
    open_end: bool,
    selection: &[&str],
) -> Result<Vec<Correction>, String> {
    let (n, r) = (said.len(), region.len());
    // lcs[i][j]: how many words the ends said[i..] and region[j..] have in common, in order.
    let mut lcs = vec![vec![0usize; r + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..r).rev() {
            lcs[i][j] = if same_ci(&said[i..=i], &region[j..=j]) {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut hunks: Vec<Hunk> = Vec::new();
    let (mut old, mut new): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0, 0);
    let mut begin = (0, 0);
    while i < n || j < r {
        if i < n && j < r && same_ci(&said[i..=i], &region[j..=j]) {
            hunks.push(Hunk {
                old: std::mem::take(&mut old),
                new: std::mem::take(&mut new),
                begin,
                end: (i, j),
            });
            i += 1;
            j += 1;
            begin = (i, j);
        } else if j >= r || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) {
            old.push(said[i]);
            i += 1;
        } else {
            new.push(region[j]);
            j += 1;
        }
    }
    hunks.push(Hunk {
        old,
        new,
        begin,
        end: (i, j),
    });

    let mut pairs = Vec::new();
    let mut unsure: Option<String> = None;
    for Hunk {
        old,
        new,
        begin,
        end,
    } in &hunks
    {
        // A pure addition or removal of words is not a correction of a heard word.
        if old.is_empty() || new.is_empty() {
            continue;
        }
        let new = narrow(new, selection);
        let open_side = (open_start && *begin == (0, 0)) || (open_end && *end == (n, r));
        if open_side && new.len() > old.len() {
            unsure.get_or_insert_with(|| ambiguity_message(old, new));
            continue;
        }
        pairs.push((phrase(old), phrase(new)));
    }
    pairs.retain(|(a, b)| !a.is_empty() && !b.is_empty());
    if let Some(message) = unsure {
        return Err(message);
    }
    if pairs.len() > MAX_PAIRS {
        return Err(format!(
            "{} corrections d'un coup, c'est trop (maximum {MAX_PAIRS})",
            pairs.len()
        ));
    }
    if pairs.is_empty() {
        return Err("Aucun mot n'a changé par rapport à la dernière dictée".into());
    }
    Ok(pairs)
}

/// The corrections made to the last dictation, found in the exact place where it was pasted.
/// None if that place cannot be found any more.
fn exact_pairs(
    said: &str,
    field: &str,
    around: &Surroundings,
    selection: &[&str],
) -> Option<Result<Vec<Correction>, String>> {
    let (said, _) = words_with_lines(said);
    let (field, _) = words_with_lines(field);
    let (start, end) = find_region(&field, &around.before, &around.after, said.len())?;
    Some(diff_pairs(
        &said,
        &field[start..end],
        around.before.is_empty(),
        around.after.is_empty(),
        selection,
    ))
}

/// Longest part of the dictation (and of the field) that may have been replaced, in words.
const MAX_SPAN: usize = 8;

/// How many unchanged words on each side are used to find the corrected part.
const ANCHOR: usize = 2;

/// A word without the punctuation around it, for comparing.
fn core(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric())
}

/// True if the dictated word `said` is stuck to other text in the field word `field`, because
/// the paste left no space after a sentence end ("bonjour." became "bonjour.lecture") or before
/// the dictation ("seule.Et" for "Et").
fn glued(said: &str, field: &str) -> bool {
    let ends_sentence_at = |text: &str| text.ends_with(['.', '!', '?', '…']);
    (ends_sentence_at(said) && field.len() > said.len() && field.starts_with(said))
        || (field.len() > said.len()
            && field.ends_with(said)
            && field[..field.len() - said.len()].ends_with(['.', '!', '?', '…']))
}

fn same_words(a: &[&str], b: &[&str]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| core(x) == core(y) || glued(x, y))
}

/// The words as a text, without punctuation at its two ends.
fn phrase(words: &[&str]) -> String {
    words
        .join(" ")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

/// True if the word ends a sentence: the corrected part cannot go on after it.
fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(['"', '\'', '»', ')', '”'])
        .ends_with(['.', '!', '?', '…'])
}

/// The words of a text, and for each one whether it starts a line.
fn words_with_lines(text: &str) -> (Vec<&str>, Vec<bool>) {
    let mut words = Vec::new();
    let mut starts_line = Vec::new();
    for line in text.lines() {
        for (i, word) in line.split_whitespace().enumerate() {
            words.push(word);
            starts_line.push(i == 0);
        }
    }
    (words, starts_line)
}

/// Looks in `field` for the dictation `said` with one part replaced, and returns
/// (what was dictated, what is in the field now). The part can change length ("Fynd de doc"
/// became "FindTheDoc"). The unchanged words around it (up to two on each side) locate it, so
/// it is found even when the field holds other text.
///
/// At the very start or end of the dictation there are no unchanged words on one side. The
/// corrected part is then delimited by the end of a sentence (. ! ?), a line break, or the
/// start or end of the field. Without any of them nothing tells where it stops.
///
/// `selection` is what the user selected: see `find_corrections`.
fn find_replacement_in(
    said: &str,
    field: &str,
    selection: &[&str],
) -> Result<(String, String), String> {
    let (said, _) = words_with_lines(said);
    let (field, line_start) = words_with_lines(field);
    let (n, m) = (said.len(), field.len());
    let mut found: Vec<(String, String)> = Vec::new();
    let mut unsure: Option<String> = None;

    for l in 0..n {
        let a = l.min(ANCHOR);
        for r in (l + 1)..=n.min(l + MAX_SPAN) {
            let b = (n - r).min(ANCHOR);
            if a == 0 && b == 0 {
                continue;
            }
            // Where the replaced part may start in the field.
            let starts: Vec<usize> = if a == 0 {
                (0..m)
                    .filter(|&j| j == 0 || line_start[j] || ends_sentence(field[j - 1]))
                    .collect()
            } else {
                (a..=m)
                    .filter(|&j| same_words(&said[l - a..l], &field[j - a..j]))
                    .collect()
            };
            for j in starts {
                for k in (j + 1)..=m.min(j + MAX_SPAN) {
                    let right_ok = if b == 0 {
                        k == m || line_start[k] || ends_sentence(field[k - 1])
                    } else {
                        k + b <= m && same_words(&said[r..r + b], &field[k..k + b])
                    };
                    if !right_ok {
                        continue;
                    }
                    let (mut old, mut new) = (&said[l..r], &field[j..k]);
                    // Words that did not change at the edges are not part of the correction.
                    while !old.is_empty() && !new.is_empty() && core(old[0]) == core(new[0]) {
                        (old, new) = (&old[1..], &new[1..]);
                    }
                    while !old.is_empty()
                        && !new.is_empty()
                        && core(old[old.len() - 1]) == core(new[new.len() - 1])
                    {
                        (old, new) = (&old[..old.len() - 1], &new[..new.len() - 1]);
                    }
                    if old.is_empty() || new.is_empty() {
                        continue;
                    }
                    let new = narrow(new, selection);
                    // Same length with an unchanged word inside: two separate corrections.
                    let two_changes = old.len() == new.len()
                        && old.len() > 2
                        && old
                            .iter()
                            .zip(new)
                            .skip(1)
                            .take(old.len() - 2)
                            .any(|(x, y)| core(x) == core(y));
                    if two_changes {
                        return Err(
                            "Plusieurs mots ont changé, je ne sais pas lequel apprendre".into()
                        );
                    }
                    // Touching the start or the end of the dictation, and longer than what was
                    // dictated: some of the words may have been added next to it.
                    if (a == 0 || b == 0) && new.len() > old.len() {
                        unsure.get_or_insert_with(|| ambiguity_message(old, new));
                        continue;
                    }
                    let pair = (phrase(old), phrase(new));
                    if !pair.0.is_empty() && !pair.1.is_empty() && !found.contains(&pair) {
                        found.push(pair);
                    }
                }
            }
        }
    }
    // A correction that contains a smaller one is the same correction seen with extra words.
    let smaller: Vec<(String, String)> = found.clone();
    found.retain(|p| {
        !smaller
            .iter()
            .any(|q| q != p && p.0.contains(&q.0) && p.1.contains(&q.1))
    });
    if found.is_empty()
        && let Some(message) = unsure
    {
        return Err(message);
    }
    match found.len() {
        0 if field_contains(&said, &field) => {
            Err("Aucun mot n'a changé par rapport à la dernière dictée".into())
        }
        0 => Err("Je ne retrouve pas la dictée dans ce champ, ou le mot corrigé est au bord de la dictée sans point ni retour à la ligne pour le délimiter".into()),
        1 => Ok(found.remove(0)),
        _ => Err("Plusieurs mots ont changé, je ne sais pas lequel apprendre".into()),
    }
}

/// True if the dictation is in the field exactly (apart from punctuation).
fn field_contains(said: &[&str], field: &[&str]) -> bool {
    field.windows(said.len()).any(|w| same_words(said, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_replacement(said: &str, field: &str) -> Result<Correction, String> {
        find_replacement_in(said, field, &[])
    }

    const SAID: &str = "Déploie la branche sur Versel puis ouvre une pull request";
    const FYND: &str = "Fynd de doc : une application qui permet de retrouver tous ses documents";

    fn pair(a: &str, b: &str) -> Result<(String, String), String> {
        Ok((a.into(), b.into()))
    }

    #[test]
    fn finds_the_corrected_word() {
        let field = "Déploie la branche sur Vercel puis ouvre une pull request";
        assert_eq!(find_replacement(SAID, field), pair("Versel", "Vercel"));
    }

    #[test]
    fn three_words_become_one_at_the_start() {
        let field = "FindTheDoc : une application qui permet de retrouver tous ses documents";
        assert_eq!(
            find_replacement(FYND, field),
            pair("Fynd de doc", "FindTheDoc")
        );
    }

    #[test]
    fn one_word_becomes_three() {
        let said = "Ouvre la FindTheDoc puis déploie";
        let field = "Ouvre la Find The Doc puis déploie";
        assert_eq!(
            find_replacement(said, field),
            pair("FindTheDoc", "Find The Doc")
        );
    }

    #[test]
    fn finds_it_among_other_text() {
        let field = "Salut Luc,\nDéploie la branche sur Vercel puis ouvre une pull request.\nMerci";
        assert_eq!(find_replacement(SAID, field), pair("Versel", "Vercel"));
    }

    #[test]
    fn punctuation_is_not_a_change() {
        let field = "Déploie la branche sur Vercel, puis ouvre une pull request.";
        assert_eq!(find_replacement(SAID, field), pair("Versel", "Vercel"));
    }

    #[test]
    fn the_last_word_is_delimited_by_the_final_period() {
        let said = "Ce composant React permet d'afficher un toggler.";
        let field = "Ce composant React permet d'afficher un bouton. Salut Luc";
        assert_eq!(find_replacement(said, field), pair("toggler", "bouton"));
    }

    #[test]
    fn the_last_word_is_delimited_by_a_line_break() {
        let said = "Ce composant React permet d'afficher un toggler";
        let field = "Ce composant React permet d'afficher un bouton\nMerci Luc";
        assert_eq!(find_replacement(said, field), pair("toggler", "bouton"));
    }

    #[test]
    fn the_first_words_are_delimited_by_the_previous_sentence() {
        let field =
            "Bonjour Luc. FindTheDoc : une application qui permet de retrouver tous ses documents";
        assert_eq!(
            find_replacement(FYND, field),
            pair("Fynd de doc", "FindTheDoc")
        );
    }

    #[test]
    fn nothing_changed() {
        assert!(
            find_replacement(SAID, SAID)
                .unwrap_err()
                .starts_with("Aucun mot")
        );
    }

    #[test]
    fn two_distant_changes_are_refused() {
        let field = "Déploie la branche sur Vercel puis ouvre une pull demande";
        assert!(
            find_replacement(SAID, field)
                .unwrap_err()
                .starts_with("Plusieurs")
        );
    }

    #[test]
    fn two_close_changes_are_refused() {
        let field = "Déploie la branche sur Vercel puis ouvrir une pull request";
        assert!(
            find_replacement(SAID, field)
                .unwrap_err()
                .starts_with("Plusieurs")
        );
    }

    #[test]
    fn unrelated_text_finds_nothing() {
        assert!(find_replacement(SAID, "Bonjour tout le monde").is_err());
    }

    fn context(before: &[&str], after: &[&str]) -> Surroundings {
        let owned = |w: &[&str]| w.iter().map(|x| x.to_string()).collect();
        Surroundings {
            before: owned(before),
            after: owned(after),
        }
    }

    fn exact(said: &str, field: &str, ctx: &Surroundings) -> Result<Vec<Correction>, String> {
        exact_pairs(said, field, ctx, &[]).expect("region found")
    }

    fn p(a: &str, b: &str) -> (String, String) {
        (a.into(), b.into())
    }

    #[test]
    fn last_word_without_delimiter_is_found_with_the_context() {
        let said = "Ce composant React permet de créer un bouton.";
        let field = "Début Ce composant React permet de créer un toggler Fin";
        let ctx = context(&["Début"], &["Fin"]);
        assert_eq!(exact(said, field, &ctx), Ok(vec![p("bouton", "toggler")]));
    }

    #[test]
    fn first_words_without_delimiter_are_found_with_the_context() {
        let field = "Salut FindTheDoc : une application qui permet de retrouver tous ses documents";
        let ctx = context(&["Salut"], &[]);
        assert_eq!(
            exact(FYND, field, &ctx),
            Ok(vec![p("Fynd de doc", "FindTheDoc")])
        );
    }

    #[test]
    fn two_distinct_changes_give_two_pairs() {
        let field = "Avant Déploie la branche sur Vercel puis ouvre une pull demande Après";
        let ctx = context(&["Avant"], &["Après"]);
        assert_eq!(
            exact(SAID, field, &ctx),
            Ok(vec![p("Versel", "Vercel"), p("request", "demande")])
        );
    }

    #[test]
    fn two_close_changes_give_two_pairs() {
        let field = "Déploie la branche sur Vercel puis ouvrir une pull request";
        let ctx = context(&[], &[]);
        assert_eq!(
            exact(SAID, field, &ctx),
            Ok(vec![p("Versel", "Vercel"), p("ouvre", "ouvrir")])
        );
    }

    #[test]
    fn four_changes_are_refused() {
        let field = "Montre la branche sur Vercel puis ouvrir une push demande";
        let ctx = context(&[], &[]);
        assert!(exact(SAID, field, &ctx).unwrap_err().contains("trop"));
    }

    #[test]
    fn lost_markers_mean_no_region() {
        let field = "Rien à voir ici Ce composant a changé complètement";
        let ctx = context(&["Début"], &["Fin"]);
        assert!(exact_pairs(SAID, field, &ctx, &[]).is_none());
    }

    #[test]
    fn a_pure_addition_is_not_a_pair() {
        let field = "Déploie la branche sur Versel puis ouvre une grosse pull request";
        let ctx = context(&[], &[]);
        assert!(exact(SAID, field, &ctx).is_err());
    }

    const LONG_LINE_BEFORE: &str = "- Aucune requête réseau, aucune télémétrie. Aède ne lit jamais en continu le texte des autres applications : la seule ";
    const LONG_LINE_AFTER: &str = " lecture est la copie de la sélection, à la demande de l'utilisateur, avec le raccourci « Apprendre un mot ».";

    #[test]
    fn a_dictation_in_the_middle_of_a_long_line_is_found() {
        let field = format!("{LONG_LINE_BEFORE}Et je te dis bonjour.{LONG_LINE_AFTER}");
        assert_eq!(
            find_replacement("Et je vous dis bonjour.", &field),
            pair("vous", "te")
        );
    }

    #[test]
    fn a_dictation_stuck_to_the_next_word_is_found() {
        // The paste left no space after the period: "bonjour.lecture" is one word.
        let field = format!(
            "{LONG_LINE_BEFORE}Et je te dis bonjour.{}",
            LONG_LINE_AFTER.trim_start()
        );
        assert_eq!(
            find_replacement("Et je vous dis bonjour.", &field),
            pair("vous", "te")
        );
    }

    #[test]
    fn the_surroundings_are_read_after_the_paste() {
        let said = "Ce composant React permet de créer un bouton.";
        let field = "Un deux trois quatre Ce composant React permet de créer un bouton. Cinq six";
        assert_eq!(
            surroundings_of(said, field),
            Ok(context(&["deux", "trois", "quatre"], &["Cinq", "six"]))
        );
    }

    #[test]
    fn the_surroundings_of_a_dictation_alone_are_empty() {
        let said = "Ce composant React permet de créer un bouton.";
        assert_eq!(surroundings_of(said, said), Ok(context(&[], &[])));
    }

    #[test]
    fn a_dictation_that_is_not_in_the_field_has_no_surroundings() {
        assert!(surroundings_of("Ce texte", "Autre chose").is_err());
    }

    #[test]
    fn find_corrections_uses_the_surroundings_when_it_can() {
        let field = "Début Ce composant React permet de créer un toggler Fin";
        let said = "Ce composant React permet de créer un bouton.";
        let around = context(&["Début"], &["Fin"]);
        assert_eq!(
            find_corrections(said, field, Some(&around), None),
            Ok(vec![p("bouton", "toggler")])
        );
    }

    #[test]
    fn find_corrections_falls_back_on_the_words_around_one_part() {
        // The surroundings are lost (the words before were changed too).
        let around = context(&["Perdu"], &["Aussi"]);
        let field = "Déploie la branche sur Vercel puis ouvre une pull request";
        assert_eq!(
            find_corrections(SAID, field, Some(&around), None),
            Ok(vec![p("Versel", "Vercel")])
        );
        assert_eq!(
            find_corrections(SAID, field, None, None),
            Ok(vec![p("Versel", "Vercel")])
        );
    }

    #[test]
    fn a_longer_correction_at_the_open_end_is_refused() {
        // The user wrote "poires" for "prunes." and added "aussi": nothing says which is which.
        let said = "J'aime bien les prunes.";
        let field = "J'aime bien les poires aussi";
        let message = find_replacement(said, field).unwrap_err();
        assert!(message.starts_with("Je ne sais pas si"), "{message}");
        let ctx = context(&[], &[]);
        let message = exact(said, field, &ctx).unwrap_err();
        assert!(message.starts_with("Je ne sais pas si"), "{message}");
    }

    #[test]
    fn keeping_the_final_period_delimits_the_correction() {
        let said = "J'aime bien les prunes.";
        let field = "J'aime bien les poires. aussi";
        assert_eq!(find_replacement(said, field), pair("prunes", "poires"));
    }

    #[test]
    fn a_shorter_or_equal_correction_at_the_open_end_is_kept() {
        let ctx = context(&[], &[]);
        assert_eq!(
            exact("J'aime bien les prunes.", "J'aime bien les poires", &ctx),
            Ok(vec![p("prunes", "poires")])
        );
        assert_eq!(
            exact(
                FYND,
                "FindTheDoc : une application qui permet de retrouver tous ses documents",
                &ctx
            ),
            Ok(vec![p("Fynd de doc", "FindTheDoc")])
        );
    }

    #[test]
    fn a_longer_correction_inside_the_dictation_is_kept() {
        let said = "Ouvre la FindTheDoc puis déploie";
        let field = "Début Ouvre la Find The Doc puis déploie Fin";
        let ctx = context(&["Début"], &["Fin"]);
        assert_eq!(
            exact(said, field, &ctx),
            Ok(vec![p("FindTheDoc", "Find The Doc")])
        );
    }

    #[test]
    fn a_selected_word_resolves_a_longer_correction_at_the_open_end() {
        let said = "J'aime bien les prunes.";
        let field = "J'aime bien les poires aussi";
        // Without the selection, the correction is refused (see the test above).
        assert!(find_corrections(said, field, None, None).is_err());
        assert_eq!(
            find_corrections(said, field, None, Some("poires")),
            Ok(vec![p("prunes", "poires")])
        );
        let around = context(&[], &[]);
        assert_eq!(
            find_corrections(said, field, Some(&around), Some("poires")),
            Ok(vec![p("prunes", "poires")])
        );
    }

    #[test]
    fn a_selection_that_is_not_a_new_word_changes_nothing() {
        let said = "J'aime bien les prunes.";
        let field = "J'aime bien les poires aussi";
        // An unchanged word, a word of nowhere, or the whole text.
        for selection in ["bien", "pommes", "J'aime bien les poires aussi"] {
            let result = find_corrections(said, field, None, Some(selection));
            assert!(result.unwrap_err().starts_with("Je ne sais pas si"));
        }
    }

    #[test]
    fn a_selection_inside_a_found_correction_narrows_it() {
        let said = "Ouvre la FindTheDoc puis déploie";
        let field = "Début Ouvre la Find The Doc puis déploie Fin";
        let around = context(&["Début"], &["Fin"]);
        assert_eq!(
            find_corrections(said, field, Some(&around), Some("The Doc")),
            Ok(vec![p("FindTheDoc", "The Doc")])
        );
    }
}
