#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagRow {
    pub id: String,
    pub name: String,
    /// Numeric plain-app `DataType` ordinal (0=DEFAULT, 1=AUDIO, 2=VIDEO,
    /// 3=IMAGE, …).
    pub kind: i32,
    pub count: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagRelationRow {
    pub tag_id: String,
    pub key: String,
}
