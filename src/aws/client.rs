use aws_config::BehaviorVersion;
use aws_config::meta::region::RegionProviderChain;

pub async fn create_clients() -> Result<(aws_sdk_ecs::Client, aws_sdk_ec2::Client), aws_sdk_ecs::Error> {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await;

    let ecs_client = aws_sdk_ecs::Client::new(&config);
    let ec2_client = aws_sdk_ec2::Client::new(&config);

    Ok((ecs_client, ec2_client))
}
