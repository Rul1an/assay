use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub enum ToolContext<'a> {
    Borrowed {
        tool_name: &'a str,
        tool_classes: &'a BTreeSet<String>,
    },
    Owned {
        tool_name: &'a str,
        tool_classes: BTreeSet<String>,
    },
}

impl<'a> ToolContext<'a> {
    pub fn tool_name(&self) -> &str {
        match self {
            Self::Borrowed { tool_name, .. } | Self::Owned { tool_name, .. } => tool_name,
        }
    }

    pub fn tool_classes(&self) -> &BTreeSet<String> {
        match self {
            Self::Borrowed { tool_classes, .. } => tool_classes,
            Self::Owned { tool_classes, .. } => tool_classes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MatchBasis {
    #[default]
    None,
    Name,
    Class,
    NameAndClass,
}

impl MatchBasis {
    pub fn as_str(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Name => Some("name"),
            Self::Class => Some("class"),
            Self::NameAndClass => Some("name+class"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResult {
    pub matched: bool,
    pub matched_classes: Vec<String>,
    pub basis: MatchBasis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRuleSelector {
    pub name: Option<String>,
    pub class: Option<String>,
}

impl ToolRuleSelector {
    pub fn new(name: Option<String>, class: Option<String>) -> Self {
        Self { name, class }
    }

    pub fn matches(&self, ctx: &ToolContext<'_>) -> MatchResult {
        let name_ok = match &self.name {
            Some(name) => ctx.tool_name() == name,
            None => true,
        };

        let mut matched_classes = Vec::new();
        let class_ok = match &self.class {
            Some(class_name) => {
                let ok = ctx.tool_classes().contains(class_name);
                if ok {
                    matched_classes.push(class_name.clone());
                }
                ok
            }
            None => true,
        };

        let basis = match (&self.name, &self.class) {
            (Some(_), Some(_)) => MatchBasis::NameAndClass,
            (Some(_), None) => MatchBasis::Name,
            (None, Some(_)) => MatchBasis::Class,
            (None, None) => MatchBasis::None,
        };

        MatchResult {
            matched: name_ok && class_ok,
            matched_classes,
            basis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_by_name_only() {
        let ctx = ToolContext::Owned {
            tool_name: "web_search",
            tool_classes: BTreeSet::new(),
        };
        let selector = ToolRuleSelector::new(Some("web_search".to_string()), None);
        let result = selector.matches(&ctx);
        assert!(result.matched);
        assert_eq!(result.basis, MatchBasis::Name);
        assert!(result.matched_classes.is_empty());
    }

    #[test]
    fn match_by_class_only() {
        let ctx = ToolContext::Owned {
            tool_name: "web_search_alt",
            tool_classes: BTreeSet::from(["sink:network".to_string()]),
        };
        let selector = ToolRuleSelector::new(None, Some("sink:network".to_string()));
        let result = selector.matches(&ctx);
        assert!(result.matched);
        assert_eq!(result.basis, MatchBasis::Class);
        assert_eq!(result.matched_classes, vec!["sink:network".to_string()]);
    }

    #[test]
    fn name_and_class_requires_both() {
        let ctx = ToolContext::Owned {
            tool_name: "web_search",
            tool_classes: BTreeSet::from(["sink:network".to_string()]),
        };
        let selector = ToolRuleSelector::new(
            Some("web_search".to_string()),
            Some("sink:network".to_string()),
        );
        let result = selector.matches(&ctx);
        assert!(result.matched);
        assert_eq!(result.basis, MatchBasis::NameAndClass);
    }

    #[test]
    fn missing_class_fails_and_selector() {
        let ctx = ToolContext::Owned {
            tool_name: "web_search",
            tool_classes: BTreeSet::new(),
        };
        let selector = ToolRuleSelector::new(
            Some("web_search".to_string()),
            Some("sink:network".to_string()),
        );
        let result = selector.matches(&ctx);
        assert!(!result.matched);
    }
}
