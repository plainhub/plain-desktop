#[derive(Clone, Debug)]
pub struct ImageEditorProjectRow {
    pub id: String,
    pub state_b64: String,
    pub thumbnail: Option<String>,
    pub canvas_width: i32,
    pub canvas_height: i32,
    pub layer_count: i32,
    pub created_at: String,
    pub updated_at: String,
}
