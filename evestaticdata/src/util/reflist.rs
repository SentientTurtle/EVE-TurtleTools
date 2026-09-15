use crate::types::ids::{TypeID, GroupID, CategoryID};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "export_hardcoded", derive(serde::Serialize))]
pub struct RefList<'a> {
    pub name: &'a str,
    #[cfg_attr(feature = "export_hardcoded", serde(rename="includedTypeIDs", skip_serializing_if="<[TypeID]>::is_empty"))]
    pub included_types: &'a [TypeID],
    #[cfg_attr(feature = "export_hardcoded", serde(rename="excludedTypeIDs", skip_serializing_if="<[TypeID]>::is_empty"))]
    pub excluded_types: &'a [TypeID],
    #[cfg_attr(feature = "export_hardcoded", serde(rename="includedGroupIDs", skip_serializing_if="<[GroupID]>::is_empty"))]
    pub included_groups: &'a [GroupID],
    #[cfg_attr(feature = "export_hardcoded", serde(rename="excludedGroupIDs", skip_serializing_if="<[GroupID]>::is_empty"))]
    pub excluded_groups: &'a [GroupID],
    #[cfg_attr(feature = "export_hardcoded", serde(rename="includedCategoryIDs", skip_serializing_if="<[CategoryID]>::is_empty"))]
    pub included_categories: &'a [CategoryID],
    #[cfg_attr(feature = "export_hardcoded", serde(rename="excludedCategoryIDs", skip_serializing_if="<[CategoryID]>::is_empty"))]
    pub excluded_categories: &'a [CategoryID],
}

impl<'a> RefList<'a> {
    pub const fn with_name(name: &'a str) -> Self {
        RefList {
            name,
            included_types: &[],
            excluded_types: &[],
            included_groups: &[],
            excluded_groups: &[],
            included_categories: &[],
            excluded_categories: &[],
        }
    }

    pub fn includes_type(&self, type_id: TypeID, group_id: GroupID, category_id: CategoryID) -> bool {
        (
            self.included_types.contains(&type_id)
                || self.included_groups.contains(&group_id)
                || self.included_categories.contains(&category_id)
        ) && !(
            self.excluded_types.contains(&type_id)
                || self.excluded_groups.contains(&group_id)
                || self.excluded_categories.contains(&category_id)
        )
    }

    pub fn includes<F: FnOnce(TypeID) -> (GroupID, CategoryID)>(&self, type_id: TypeID, f: F) -> bool {
        let (group_id, category_id) = f(type_id);
        self.includes_type(type_id, group_id, category_id)
    }

    #[cfg(feature = "sde_load")]
    pub fn to_typelist(self) -> crate::sde::load::TypeList {
        crate::sde::load::TypeList {
            typeListID: 0,
            displayName: None,
            displayDescription: None,
            name: "".to_string(),
            includedTypeIDs: vec![],
            excludedTypeIDs: vec![],
            includedGroupIDs: vec![],
            excludedGroupIDs: vec![],
            includedCategoryIDs: vec![],
            excludedCategoryIDs: vec![],
        }
    }

    #[allow(clippy::needless_lifetimes)]
    pub fn flatten<'b,
        FT: Fn(TypeID) -> (GroupID, CategoryID),
        FG: Fn(GroupID) -> (CategoryID, &'b [TypeID]),
        FC: Fn(CategoryID) -> &'b [GroupID]
    >(&'b self, type_info: FT, group_info: FG, category_info: FC) -> Vec<TypeID> {
        let mut buf = Vec::with_capacity(self.included_types.len());

        for type_id in self.included_types {
            if !self.excluded_types.contains(type_id) {
                let (group, category) = type_info(*type_id);
                if !(self.excluded_groups.contains(&group) || self.excluded_categories.contains(&category)) {
                    buf.push(*type_id);
                }
            }
        }

        for group in self.included_groups {
            if !self.excluded_groups.contains(group) {
                let (category, types) = group_info(*group);
                if !self.excluded_categories.contains(&category) {
                    for type_id in types {
                        if !self.excluded_types.contains(type_id) {
                            buf.push(*type_id);
                        }
                    }
                }
            }
        }

        for category in self.included_categories {
            if !self.excluded_categories.contains(category) {
                for group in category_info(*category) {
                    if !self.excluded_groups.contains(group) {
                        let (_, types) = group_info(*group);
                        for type_id in types {
                            if !self.excluded_types.contains(type_id) {
                                buf.push(*type_id);
                            }
                        }
                    }
                }
            }
        }

        buf
    }
}
