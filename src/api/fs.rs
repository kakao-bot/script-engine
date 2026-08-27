use rquickjs::function::Func;
use rquickjs::{Ctx, Value};

pub fn install<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<()> {
    let fs = rquickjs::Object::new(ctx.clone())?;

    fs.set(
        "read",
        Func::from(|ctx: Ctx<'js>, path: String| {
            promising(ctx, async move { tokio::fs::read_to_string(path).await })
        }),
    )?;

    fs.set(
        "write",
        Func::from(|ctx: Ctx<'js>, path: String, contents: String| {
            promising(ctx, async move {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(path, contents)
                    .await
                    .map(|()| String::new())
            })
        }),
    )?;

    fs.set(
        "append",
        Func::from(|ctx: Ctx<'js>, path: String, contents: String| {
            promising(ctx, async move {
                use tokio::io::AsyncWriteExt;
                let mut file = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await?;
                file.write_all(contents.as_bytes()).await?;
                Ok(String::new())
            })
        }),
    )?;

    fs.set(
        "exists",
        Func::from(|path: String| std::path::Path::new(&path).exists()),
    )?;

    fs.set(
        "remove",
        Func::from(|ctx: Ctx<'js>, path: String| {
            promising(ctx, async move {
                tokio::fs::remove_file(path).await.map(|()| String::new())
            })
        }),
    )?;

    fs.set(
        "mkdir",
        Func::from(|ctx: Ctx<'js>, path: String| {
            promising(ctx, async move {
                tokio::fs::create_dir_all(path)
                    .await
                    .map(|()| String::new())
            })
        }),
    )?;

    fs.set(
        "list",
        Func::from(|ctx: Ctx<'js>, path: String| {
            let (promise, resolve, reject) = ctx.promise()?;
            ctx.spawn(async move {
                match entries(path).await {
                    Ok(names) => {
                        let _ = resolve.call::<_, Value>((names,));
                    }
                    Err(error) => {
                        let _ = reject.call::<_, Value>((error.to_string(),));
                    }
                }
            });
            Ok::<_, rquickjs::Error>(promise)
        }),
    )?;

    ctx.globals().set("fs", fs)
}

fn promising<'js, F>(ctx: Ctx<'js>, work: F) -> rquickjs::Result<rquickjs::Promise<'js>>
where
    F: Future<Output = std::io::Result<String>> + 'js,
{
    let (promise, resolve, reject) = ctx.promise()?;
    ctx.spawn(async move {
        match work.await {
            Ok(value) => {
                let _ = resolve.call::<_, Value>((value,));
            }
            Err(error) => {
                let _ = reject.call::<_, Value>((error.to_string(),));
            }
        }
    });
    Ok(promise)
}

async fn entries(path: String) -> std::io::Result<Vec<String>> {
    let mut reading = tokio::fs::read_dir(path).await?;
    let mut names = Vec::new();
    while let Some(entry) = reading.next_entry().await? {
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}
