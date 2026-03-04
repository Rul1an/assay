use super::tool_match::ToolContext;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolTaxonomy {
    #[serde(default)]
    pub tool_classes: HashMap<String, BTreeSet<String>>,
}

impl ToolTaxonomy {
    pub fn classes_for(&self, tool_name: &str) -> BTreeSet<String> {
        self.tool_classes
            .get(tool_name)
            .cloned()
            .unwrap_or_default()
    }

    pub fn context<'a>(&'a self, tool_name: &'a str) -> ToolContext<'a> {
        ToolContext::Owned {
            tool_name,
            tool_classes: self.classes_for(tool_name),
        }
    }
}
