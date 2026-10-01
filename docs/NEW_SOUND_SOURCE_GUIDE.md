# 新音源开发指南

本文说明如何在本仓库新增一个物理建模音源（例如贝斯、鼓、弦乐或其他乐器），以及哪些已有 crate 可以复用。目标是让新音源拥有独立、可测试的物理模型，同时复用通用 DSP、编辑器和 preset 能力。

> **维护声明：** 本文允许并鼓励持续更新。如果发现内容有错误、已经过时或有所遗漏，请直接修订本文，并尽可能依据当前代码和测试验证更新后的说明。

## 1. 架构原则

- **每种乐器拥有自己的物理模型和引擎。** 新增 `crates/physics-<instrument>`，不要把不同乐器的物理过程硬塞进钢琴或吉他 crate。
- **共享通用机制，不共享不相同的物理含义。** 滤波器、模态离散化、噪声、GUI 控件和 preset 管理可以复用；琴弦、簧片、膜片、琴体等模型若物理意义或参数不同，就留在乐器 crate 中。
- **先做可离线验证的引擎，再接插件和 GUI。** 物理模型能单独渲染、测试和测量后，再接实时事件、参数和界面。
- **实时处理路径以零分配为目标。** 初始化时准备引擎和工作区，音频回调只处理已有状态和缓冲区。

仓库已有物理建模背景文档位于 [`docs/design/`](design/)；可对照 [Bass 设计](design/Physical%20Modeling%20Bass%20Synthesis.md)、[Drum 设计](design/Physical%20Modeling%20Drum%20Synthesizer.md) 和 [Bass/Drum 实现范围与限制](PHYSICAL_MODELING_STATUS.md)。新增音源前，先记录其激励方式、主要状态、耦合路径、阻尼、离散化方法和预期听感；不要只从 GUI 或插件参数开始。明确区分数值稳定、物理近似和经测量校准的声学准确度；没有实测数据时，不要宣称模型已匹配特定乐器。

## 2. 可复用 crate

| crate | 适合复用 | 不应该放进去的内容 |
|---|---|---|
| [`physics-dsp`](../crates/physics-dsp) | `Biquad`、`ModalTransition`、`OverdampedPolicy`、`XorShift32/64` 等通用、无分配 DSP 原语 | 只适用于某种乐器的琴桥、琴体、激励器或调音规则 |
| [`physics-ui`](../crates/physics-ui) | Vizia 参数滑杆、离散选择器、preset 面板、语言辅助、字体加载、Skia 兼容层 | 新乐器的专属可视化、参数语义和编辑器布局 |
| [`physics-presets`](../crates/physics-presets) | `Preset`、`PresetManager`、`UndoManager`、preset JSON 的通用读写和历史记录 | 新乐器自己的参数定义、参数 ID 和声音设计 |

### `physics-dsp`

在乐器 crate 的 `Cargo.toml` 中加入：

```toml
physics-dsp = { path = "../physics-dsp" }
```

例如，所有使用二阶模态离散化的实现都可以调用 `ModalTransition`，但应按各自物理模型选择 `OverdampedPolicy`。钢琴和吉他已经保留了不同的过阻尼策略，不要仅为了代码相似就强行统一声音行为。

### `physics-ui`

常用入口包括：

- `parameter_slider`、`discrete_selector`：通用参数编辑控件。
- `preset_panel`：preset 导航、保存、撤销/重做及语言操作的共用面板。
- `Language`、`ui_text`、`setup_vizia_fonts`：双语和字体支持。
- `skia_compat`：跨 Skia 版本的绘图兼容。

依赖声明：

```toml
physics-ui = { path = "../physics-ui" }
```

`add_base_theme()` 加载 `physics-ui/src/theme.css` 中的共享基础主题；乐器 crate 只为专属视图和布局添加必要的局部样式覆盖。专属视图、布局和 i18n 文案仍放在乐器 crate 中，不要复制共享 preset 面板或通用参数控件。共享 preset 面板的禁用条件是：Rename、Overwrite、Delete 仅对用户 preset 启用；Undo/Redo 分别取决于历史栈；Save As 始终可用。扩展面板时，保持这些状态与视觉样式一致。

### `physics-presets`

可以复用 `PresetManager`、`Preset` 和 `UndoManager`。新乐器需要定义自己的参数 ID 和 factory presets；通常在 `assets/presets/<instrument>/` 放置 JSON，并在 `physics-presets` 中增加对应的 factory preset 加载函数，再由新插件传入 `PresetManager::new("<instrument>", presets)`。

Preset 中的参数键必须与插件参数 ID 对应。参数一旦发布给 DAW 或写入用户 preset，就视为持久化格式；重命名或改变含义前要考虑兼容迁移。

`UndoManager` 支持单参数手势和批量 preset 变更。滑块的 Begin/Set/End 事件都要正确接入：参数包装器的 `Param::value()` 在 End 事件时可能仍是旧值，因此应记录 `SetParameterNormalized` 中的最新请求值。若支持右键恢复，拖动中取消应结束当前手势但不提交历史；完成手势后恢复则应移除对应的最新单参数 Undo 记录，并仍以 Begin/Set/End 通知宿主。

## 3. 推荐 crate 结构

可以参考现有 `physics-piano`、`physics-guitar`，按新乐器实际需要增删模块：

```text
crates/physics-<instrument>/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── engine.rs             # 乐器级编排、事件和渲染入口
│   ├── core/                 # 乐器专属物理部件
│   ├── params/               # 参数生成、调音表或标定数据
│   ├── dsp/                  # 乐器专属滤波/卷积/辐射模型（若需要）
│   ├── gui/                  # Vizia 编辑器及专属可视化
│   ├── nice_plugin.rs        # 参数、宿主事件、CLAP/VST3/Standalone 适配
│   ├── presets.rs            # 可选：乐器侧的 preset 映射
│   └── bin/
│       ├── cli.rs            # 可选：离线渲染、分析和基准测试
│       └── standalone.rs     # 独立播放程序
└── tests/                    # 引擎级物理与实时回归测试
```

以上目录树只是一个可调整的模板，并非仓库强制布局：Bass、Drum 使用 `src/gui.rs` 编排编辑器并把专属视图放在 `src/gui/`；Piano、Guitar 则采用 `src/gui/editor.rs`。测试也可以就近放在模块的 `#[cfg(test)]` 中，不必为了符合模板而创建空的 `tests/` 目录。

新 crate 要加入根目录 `Cargo.toml` 的 workspace `members`。共享依赖按需引入：

```toml
physics-dsp = { path = "../physics-dsp" }
physics-ui = { path = "../physics-ui" }
physics-presets = { path = "../physics-presets" }
```

对照现有 crate 添加插件和图形依赖。不要无条件复制 `realfft`、`hound`、`rayon` 等依赖；只有模型或功能真正需要时再加。

## 4. 推荐开发步骤

### 第一步：写清楚模型和参数

在 `docs/design/` 中说明至少以下内容：

1. 输入/激励是什么，音符如何开始、持续和结束。
2. 状态变量及单位是什么，子系统如何耦合，能量从何处输入和耗散。
3. 连续模型如何离散化；怎样保证数值稳定，如何处理过阻尼和高频混叠。
4. 哪些参数可由用户控制，其默认值、范围、单位、平滑方式和稳定的参数 ID。
5. 用哪些客观指标和听感用例判断正确性，例如基频、衰减时间、能量变化、瞬态和不同采样率下的输出；记录哪些结论来自仿真测试，哪些来自实测 FRF/STFT 或听感评估。
6. 说明模型保真度与已知限制：降阶模态、集中参数或启发式耦合不等同于完整连续体 PDE，也不自动意味着与真实乐器匹配。

### 第二步：独立完成 DSP 与离线渲染

先实现并测试乐器核心状态和 `engine.rs`。引擎接口应让插件层可以提交带采样偏移的事件，并将音频写入调用者提供的输出缓冲区。GUI、文件读写、DAW 参数和平台音频设备不要混进核心物理模块。

给引擎提供确定的初始化配置（采样率、最大缓冲长度、模态数、复音数等），并提供适当的重置、事件和渲染入口。若需要创建 voice、滤波器或工作区，应在引擎初始化或专门的 prepare 阶段完成，而不是等第一条 Note On 到来才临时创建。

### 第三步：接入宿主和 standalone

参考现有 `nice_plugin.rs`：

- 用 nice-plug 定义参数，并给每个参数设置稳定、唯一的 `#[id = "..."]`。
- 在 `initialize` 中读取采样率和最大 block size，创建/准备引擎及 scratch buffers。
- 在 `process` 中转换 MIDI/宿主事件，保留采样偏移，调用引擎并写出音频。
- `reset`、参数自动化、预设加载和 Note Off 都要有明确的状态语义。
- 若暴露 CLAP/VST3，按现有 crate 使用的导出宏和构建设置配置 `lib.rs`。只有确实需要原始 CLAP C ABI 能力时才参考 piano 的 `clap_plugin.rs`；通常不必为新乐器复制这层。
- standalone 与离线 CLI 是适配层，不应维护另一份物理模型。

### 第四步：接入编辑器和 preset

在编辑器中使用 `physics-ui` 的通用控件和 preset 面板；乐器专属视图单独实现。主题需要覆盖普通、hover、pressed、checked 和 disabled 状态，并在 GUI 中检查动态状态切换。共享面板的 `btn-disabled` 样式用于让不同禁用来源保持一致；新增禁用按钮时，同时核对禁用逻辑、点击命中和文字对比度。布局 spacer 若可能覆盖相邻按钮，应设为 `PointerEvents::None`。名称输入框也应验证点击非焦点控件后能正常失焦。特别是 `ParamButton` 的选中状态使用 `param-button:checked`，文字是内部 `label`；只设置父按钮颜色可能不会改变子标签颜色，必须检查实际对比度。

### 第五步：按层验证

建议逐层增加测试：

- **物理单元测试**：离散系数有限、状态不爆炸、能量/衰减合理、边界参数有效。
- **引擎测试**：Note On/Off、重复触发、复音/voice stealing、参数变更、重置及不同采样率。
- **实时测试**：对初始化后的 note/process 路径加入无分配回归测试；确认 GUI 事件、事件暂存和输出事件处理也不会在音频线程分配或释放。
- **端到端验证**：离线渲染、听感试听、standalone 快速演奏，以及 plugin host 自动化/预设往返。

常用命令（以下以 `physics-drum` 为例；新 crate 应替换成其 `Cargo.toml` 中的 package/bin 名称）：

```bash
cargo fmt --all --check
cargo check -p physics-drum --all-targets
cargo nextest run -p physics-drum
cargo nextest run --workspace --no-fail-fast
cargo run --release -p physics-drum --bin physics-drum-rs -- --help
cargo run --release -p physics-drum --bin physics-drum-standalone
cargo xtask bundle physics-drum --release
```

本仓库优先使用 `cargo nextest` 运行测试；未安装时可用 `cargo test` 替代。Bass/Drum 的 CLI bin 分别为 `physics-bass-rs`、`physics-drum-rs`，standalone bin 分别为 `physics-bass-standalone`、`physics-drum-standalone`。最后一条只适用于已经配置插件导出的 crate。

## 5. 实时音频注意事项

音频 callback 必须短、确定、无阻塞；低延迟 buffer 下更容易暴露问题。

**不要在 callback 中：**

- 创建/扩容/克隆 `Vec`、`String`、`HashMap` 或大型 voice/modal 对象，也不要在处理消息时触发隐式的释放。
- 做文件或网络 I/O、打印日志、等待锁、阻塞发送/接收，或取得 UI 线程持有的锁。
- 每个 block 都对未变化的参数重新计算整套滤波/模态系数。

**推荐做法：**

- `initialize`/prepare 时预分配 buffers、voices、maps、队列和模态工作区；设置容量时按最大 block size、复音数和事件峰值预算。
- UI→audio 使用有界队列（例如 `crossbeam_channel::bounded`）或原子参数；队列满时要定义明确策略。无界队列的 `try_recv()` 在取尽事件时也可能释放内部内存块，因此不能仅检查发送端是否分配。
- 让音频线程独占引擎状态；用原子或无锁快照传递简单控制值。复杂配置应在非实时线程准备后通过安全的状态交换机制切换。
- 对参数变更做差异检查、平滑或分批更新，避免一个 block 突然重算所有 voice。
- 用 `nice-assert-no-alloc`（四个现有音源 crate 都将其作为 dev-dependency）及插件的实时分配检查来覆盖真正的 callback 路径。只测核心 `note_on()` 不够；还要测试宿主事件和 GUI 队列的排空路径。

## 6. 容易踩的坑

1. **不要为了复用而合并物理模型。** 共享 `ModalTransition` 不代表不同乐器必须使用同一过阻尼策略、激励模型或参数表。
2. **不要只以 GUI/离线测试代替实时验证。** 回调里的隐式分配和释放常常只在输入事件到来时出现。
3. **考虑采样率和最大 block size。** 避免固定假设 44.1/48 kHz 或固定每次回调长度；测试至少两个常见采样率和不同 block size。
4. **参数 ID 是持久化接口。** DAW automation、宿主状态和 preset 都依赖稳定 ID；改变范围/单位时也要检查旧状态映射。
5. **UI 状态和音频状态不要共用锁。** GUI 可以发命令和读取原子快照，但不要直接拿着 UI mutex 在音频线程里等待。
6. **保持小而深的引擎接口。** 插件适配层负责宿主协议，核心引擎负责事件到音频的行为；不要让每个 GUI 控件直接操作一堆物理部件。
7. **核对现有文档和实现。** 新增架构说明应以当前 crate 的 `Cargo.toml` 和 `src/` 为准，并符合插件 GUI 现用的 Vizia/nice-plug 实现。
8. **区分回归通过与声学校准。** 有限输出、稳定衰减和自动化测试通过，只能说明对应测试覆盖范围内的行为正常；没有测量基准和听感评估时，不要把它们描述为真实乐器匹配结果。
