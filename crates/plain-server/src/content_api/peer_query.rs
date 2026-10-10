use async_graphql::{
    Context, ContextSelectionSet, ObjectType, OutputType, Positioned, ServerResult, Value,
    parser::types::Field,
    registry::{self, MetaTypeId},
    resolver_utils::{ContainerType, resolve_container},
};
use std::borrow::Cow;

// The published mobile peer schema has an empty Query object.
pub(super) struct Query;
impl ContainerType for Query {
    async fn resolve_field(&self, _: &Context<'_>) -> ServerResult<Option<Value>> {
        Ok(None)
    }
}
impl OutputType for Query {
    fn type_name() -> Cow<'static, str> {
        Cow::Borrowed("Query")
    }
    fn create_type_info(registry: &mut registry::Registry) -> String {
        registry.create_output_type::<Self, _>(MetaTypeId::Object, |_| registry::MetaType::Object {
            name: "Query".into(),
            description: None,
            fields: Default::default(),
            cache_control: Default::default(),
            extends: false,
            shareable: false,
            resolvable: true,
            keys: None,
            visible: None,
            inaccessible: false,
            interface_object: false,
            tags: Default::default(),
            is_subscription: false,
            rust_typename: Some(std::any::type_name::<Self>()),
            directive_invocations: Default::default(),
            requires_scopes: Default::default(),
        })
    }
    async fn resolve(
        &self,
        ctx: &ContextSelectionSet<'_>,
        _: &Positioned<Field>,
    ) -> ServerResult<Value> {
        resolve_container(ctx, self).await
    }
}
impl ObjectType for Query {}
