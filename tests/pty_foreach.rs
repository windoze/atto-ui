use chatty_test_host::PtyTestHost;
use std::time::Duration;

#[test]
fn test_foreach_simple_list() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_foreach_demo");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    // 等待水果列表渲染
    host.wait_for_text("Fruit List", Duration::from_secs(2))?;
    host.wait_for_text("0. Apple", Duration::from_secs(2))?;
    host.wait_for_text("1. Banana", Duration::from_secs(2))?;
    host.wait_for_text("2. Cherry", Duration::from_secs(2))?;
    host.wait_for_text("3. Durian", Duration::from_secs(2))?;

    host.send_ctrl('q').expect("quit");

    Ok(())
}

#[test]
fn test_foreach_dynamic_add() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_foreach_demo");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    // 等待初始渲染
    host.wait_for_text("Fruit List", Duration::from_secs(2))?;
    host.wait_for_text("3. Durian", Duration::from_secs(2))?;

    // 按 'a' 添加新水果
    host.send_str("a").expect("send 'a'");
    std::thread::sleep(Duration::from_millis(200));

    // 验证新元素出现
    host.wait_for_text("4. Elderberry", Duration::from_secs(2))?;

    host.send_ctrl('q').expect("quit");

    Ok(())
}

#[test]
fn test_foreach_dynamic_remove() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_foreach_demo");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    // 等待初始渲染
    host.wait_for_text("Fruit List", Duration::from_secs(2))?;
    host.wait_for_text("0. Apple", Duration::from_secs(2))?;
    host.wait_for_text("1. Banana", Duration::from_secs(2))?;

    // 按 'r' 删除第一个元素
    host.send_str("r").expect("send 'r'");
    std::thread::sleep(Duration::from_millis(200));

    // 验证第一个元素被删除，其他元素的索引更新
    let screen = host.screen_contents()?;
    // Apple 应该消失，Banana 变成 0. Banana
    assert!(screen.contains("0. Banana"));
    assert!(screen.contains("1. Cherry"));

    host.send_ctrl('q').expect("quit");

    Ok(())
}
