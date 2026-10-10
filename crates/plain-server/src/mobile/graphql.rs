use crate::mobile::contacts::resolvers::ContactsMutation;
use crate::mobile::contacts::resolvers::ContactsQuery;
use crate::mobile::packages::resolvers::PackagesMutation;
use crate::mobile::packages::resolvers::PackagesQuery;
use crate::mobile::phone_calls::resolvers::CallsMutation;
use crate::mobile::phone_calls::resolvers::CallsQuery;
use crate::mobile::sms::resolvers::SmsMutation;
use crate::mobile::sms::resolvers::SmsQuery;
use async_graphql::{EmptySubscription, MergedObject, SchemaBuilder};
#[derive(MergedObject, Default)]
pub struct Query(PackagesQuery, ContactsQuery, SmsQuery, CallsQuery);
#[derive(MergedObject, Default)]
pub struct Mutation(
    PackagesMutation,
    ContactsMutation,
    SmsMutation,
    CallsMutation,
);
pub(crate) fn configure(
    builder: SchemaBuilder<
        crate::graphql::schemas::public::Query,
        crate::graphql::schemas::public::Mutation,
        EmptySubscription,
    >,
    runtime: std::sync::Arc<super::Runtime>,
) -> SchemaBuilder<
    crate::graphql::schemas::public::Query,
    crate::graphql::schemas::public::Mutation,
    EmptySubscription,
> {
    builder.data(runtime.mms.clone()).data(std::sync::Arc::new(
        super::phone_calls::geo::Runtime::default(),
    ))
}
