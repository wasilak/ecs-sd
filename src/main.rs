use aws_config::BehaviorVersion;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ecs::{Client, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await;

    let client = Client::new(&config);

    let clusters_list = vec![
        "service-platform-default".to_string(),
        "arn:aws:ecs:eu-west-1:723255075185:cluster/service-platform-default".to_string(),
    ];

    let clusters = show_clusters(&client, &clusters_list).await?;

    list_tasks_in_cluster(&client, clusters).await;

    Ok(())
}

async fn show_clusters(
    client: &aws_sdk_ecs::Client,
    clusters_list: &[String],
) -> Result<Option<Vec<aws_sdk_ecs::types::Cluster>>, aws_sdk_ecs::Error> {
    // let resp = client.list_clusters().send().await?;

    let clusters = client
        .describe_clusters()
        .set_clusters(Some(clusters_list.into()))
        .send()
        .await?;

    Ok(Some(clusters.clusters().to_vec()))
}

async fn list_tasks_in_cluster(
    client: &aws_sdk_ecs::Client,
    cluster: Option<Vec<aws_sdk_ecs::types::Cluster>>,
) {
    if let Some(clusters) = cluster {
        for cluster in clusters {
            println!("  ARN:  {}", cluster.cluster_arn().unwrap());
            println!("  Name: {}", cluster.cluster_name().unwrap());

            let tasks_list = client
                .list_tasks()
                .set_cluster(cluster.cluster_arn().map(|s| s.to_string()))
                .send()
                .await
                .unwrap();

            let tasks = client
                .describe_tasks()
                .set_cluster(cluster.cluster_arn().map(|s| s.to_string()))
                .set_tasks(Some(tasks_list.task_arns().to_vec()))
                .send()
                .await
                .unwrap();

            for task in tasks.tasks() {
                println!("    Task ARN: {}", task.task_arn().unwrap());
                println!(
                    "    Task Definition ARN: {}",
                    task.task_definition_arn().unwrap()
                );
                println!("    Last Status: {}", task.last_status().unwrap());

                let task_definition = client
                    .describe_task_definition()
                    .set_task_definition(task.task_definition_arn().map(|s| s.to_string()))
                    .send()
                    .await
                    .unwrap();

                for container_def in task_definition
                    .task_definition()
                    .unwrap()
                    .container_definitions()
                {
                    println!("      Container Name: {}", container_def.name().unwrap());
                    println!("      Image: {}", container_def.image().unwrap());
                    println!("      Environment Variables:");
                    for env_var in container_def.environment() {
                        println!(
                            "        {}: {}",
                            env_var.name().unwrap(),
                            env_var.value().unwrap()
                        );
                    }
                }
            }
        }
    }
}
