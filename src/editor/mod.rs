//! This module contains the logic for editing XML files without using Serde this is done using the edit-xml crate.
//!
//! Why not use Serde?
//! Using this will keep structure and comments in the XML file.
//!  This is important for pom files as comments are used to describe the purpose of the element.
use std::{fmt::Display, path::PathBuf};

use edit_xml::{Document, EditXMLError, Element};
use thiserror::Error;
use utils::MissingElementError;

use crate::{
    pom::{
        DependencyBuilderError, ParentBuilderError, PluginBuilderError, RepositoryBuilderError,
        SubRepositoryRulesBuilderError,
    },
    settings::{MirrorBuilderError, ServerBuilderError},
};
pub mod utils;

#[derive(Debug, Error)]
pub enum XMLEditorError {
    #[error(transparent)]
    MissingElement(#[from] MissingElementError),
    #[error("Unexpected Element Type. Expected {expected}, found {found}")]
    UnexpectedElementType {
        expected: &'static str,
        found: String,
    },
    #[error(transparent)]
    InvalidValue(#[from] InvalidValueError),
    #[error(transparent)]
    EditXMLError(#[from] EditXMLError),
    #[error("Error During Validation of type {pom_type} {error}")]
    ValidationError {
        pom_type: &'static str,
        error: String,
    },
}
macro_rules! builder_err {
    ($error_type:ident, $pom_type:literal) => {
        impl From<$error_type> for XMLEditorError {
            fn from(value: $error_type) -> Self {
                match value {
                    $error_type::UninitializedField(missing_field) => {
                        XMLEditorError::MissingElement(MissingElementError(missing_field))
                    }
                    $error_type::ValidationError(other) => XMLEditorError::ValidationError {
                        pom_type: $pom_type,
                        error: other,
                    },
                }
            }
        }
    };
}
builder_err!(DependencyBuilderError, "Dependency");
builder_err!(PluginBuilderError, "Plugin");
builder_err!(ParentBuilderError, "Parent");
builder_err!(ServerBuilderError, "Server");
builder_err!(MirrorBuilderError, "Mirror");
builder_err!(SubRepositoryRulesBuilderError, "SubRepositoryRules");
builder_err!(RepositoryBuilderError, "Repository");
pub trait HasElementName {
    fn element_name() -> &'static str;
}

pub trait ElementConverter: Sized {
    fn from_element(element: Element, document: &Document) -> Result<Self, XMLEditorError>;

    fn into_element(self, document: &mut Document) -> Result<Element, XMLEditorError>
    where
        Self: HasElementName,
    {
        let element = Element::new(document, Self::element_name());
        let children = self.into_children(document)?;
        for child in children {
            element.push_child(document, child.into())?;
        }
        Ok(element)
    }

    fn into_children(self, document: &mut Document) -> Result<Vec<Element>, XMLEditorError>;
}
/// Used Internally for updating a type of element.
pub trait UpdatableElement: ElementConverter {
    /// Checks if the current element is the same as the other element.
    /// Some implementations may only check a subset of fields. Such as [Dependency](crate::pom::Dependency) only checking the group id and artifact id.
    fn is_same_item(&self, other: &Self) -> bool;
    // Updates the element with the current element.
    fn update_element(
        &self,
        element: Element,
        document: &mut Document,
    ) -> Result<(), XMLEditorError>;
    /// Replaces all children of the element with the children of the current element.
    fn replace_all_elements(
        self,
        element: Element,
        document: &mut Document,
    ) -> Result<(), XMLEditorError> {
        element.clear_children(document);
        let children = self.into_children(document)?;
        for child in children {
            element.push_child(document, child.into())?;
        }
        Ok(())
    }
}
/// Used Internally for updating list type structures such as dependencies and plugins.
pub trait ChildOfListElement: ElementConverter {
    fn parent_element_name() -> &'static str;
}
#[derive(Debug, Error)]
pub struct InvalidValueError {
    pub expected: &'static str,
    pub found: String,
    pub source_element: Option<&'static str>,
}
impl Display for InvalidValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(source_element) = self.source_element {
            write!(
                f,
                "Invalid Value for {}. Expected {}, found {}",
                source_element, self.expected, self.found
            )
        } else {
            write!(
                f,
                "Invalid Value. Expected {}, found {}",
                self.expected, self.found
            )
        }
    }
}

pub trait PomValue: Sized {
    fn from_string_for_editor(value: String) -> Result<Self, InvalidValueError> {
        Self::from_str_for_editor(&value)
    }

    fn from_str_for_editor(value: &str) -> Result<Self, InvalidValueError>;

    fn to_string_for_editor(&self) -> String;

    fn from_element(element: Element, document: &Document) -> Result<Self, XMLEditorError>
    where
        Self: Sized,
    {
        let value = element.text_content(document);
        Self::from_str_for_editor(&value).map_err(|e| e.into())
    }
}

impl PomValue for bool {
    fn from_str_for_editor(value: &str) -> Result<Self, InvalidValueError> {
        match value {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(InvalidValueError {
                expected: "true or false",
                found: value.to_string(),
                source_element: None,
            }),
        }
    }

    fn to_string_for_editor(&self) -> String {
        self.to_string()
    }
}

impl PomValue for String {
    fn from_str_for_editor(value: &str) -> Result<Self, InvalidValueError> {
        Ok(value.to_string())
    }
    fn from_string_for_editor(value: String) -> Result<Self, InvalidValueError> {
        Ok(value)
    }
    fn to_string_for_editor(&self) -> String {
        self.clone()
    }
}
macro_rules! pom_value_num {
    (
        $(
            $type:ty
        ),*
    ) => {
        $(
            impl PomValue for $type {
                fn from_str_for_editor(value: &str) -> Result<Self, InvalidValueError> {
                    value.parse().map_err(|_| InvalidValueError {
                        expected: "A number",
                        found: value.to_string(),
                        source_element: None,
                    })
                }

                fn to_string_for_editor(&self) -> String {
                    self.to_string()
                }
            }
        )*
    };
}
pom_value_num!(usize, u8, u16, u32, u64, u128, isize, i8, i16, i32, i64, i128, f32, f64);

impl PomValue for PathBuf {
    fn from_str_for_editor(value: &str) -> Result<Self, InvalidValueError> {
        Ok(PathBuf::from(value))
    }

    fn to_string_for_editor(&self) -> String {
        // this is probably not the best way to do this.
        self.to_string_lossy().to_string()
    }
}
