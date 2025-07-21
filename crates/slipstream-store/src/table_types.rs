// Table name constants for compile-time safety
pub const EPISODES_TABLE: &str = "episodes";
pub const ENTITIES_TABLE: &str = "entities";
pub const EDGES_TABLE: &str = "edges";
pub const COMMUNITIES_TABLE: &str = "communities";

// Example of how commands would use this:
// 
// pub struct SaveEntities<const TABLE: &'static str = ENTITIES_TABLE> {
//     pub entities: Vec<Arc<Entity>>,
//     pub embedding_dimensions: i32,
// }
//
// impl<const TABLE: &'static str> DatabaseCommand for SaveEntities<TABLE> {
//     type Output = usize;
//     type SaveData = Entity;
//
//     fn to_operation(&self) -> DatabaseOperation<Self::Output, Self::SaveData> {
//         DatabaseOperation::Mutation(MutationOperation::Bulk {
//             table: TABLE,  // Compile-time constant!
//             data: self.entities.clone(),
//             graph_context: (),
//             meta_context: self.embedding_dimensions,
//             cypher: "UNWIND $items AS item CREATE (n:Entity {uuid: item.uuid, name: item.name})",
//             mode: SaveMode::MergeInsert { on_columns: vec!["uuid".to_string()] },
//             transformer: Box::new(|count| count),
//         })
//     }
// }