// SPDX-License-Identifier: AGPL-3.0-or-later
//! Модель графа SoundBimba (файл проекта): узлы, провода, проверка, чтение и запись JSON.
//! Узлы не знают о звуке — граф можно открыть и без звуковых устройств.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Самая новая версия схемы файла проекта, которую понимает программа. 2 — есть микросхемы
/// (0.5.0); файл без микросхем по-прежнему пишется версией 1, и его открывает и 0.4.
pub const SCHEMA: u32 = 2;
/// Версия файла без микросхем.
pub const SCHEMA_BASE: u32 = 1;
/// Глубже — явно испорченный файл (микросхема в микросхеме в …).
pub const MAX_CHIP_DEPTH: usize = 8;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    pub schema: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: NodeKind,
    pub pos: [f64; 2],
    #[serde(default)]
    pub params: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    /// Звук приложения (захват процесса по имени exe).
    AppSource,
    /// Микрофон.
    MicInput,
    /// Кнопка-звук.
    SoundButton,
    /// Громкость.
    Volume,
    /// Компрессор: громкое тише, затем всё громче (ровный голос без перегруза).
    Compressor,
    /// Морфер голоса: высота, «робот», «рация», эхо.
    VoiceMorpher,
    /// Шумоподавление (RNNoise): оставляет голос, убирает фон.
    Denoise,
    /// Выход «Наушники» (реальное устройство вывода).
    SpeakerOut,
    /// Выход «Виртуальный микрофон» (динамик виртуального кабеля).
    VirtualOut,
    /// Микросхема: внутри целая схема (`params.inner` — узлы и провода), снаружи — ножки:
    /// входы — её узлы «Вход микросхемы», выходы — узлы «Выход микросхемы».
    Chip,
    /// Вход микросхемы (ножка слева); внутри схемы — как источник.
    ChipIn,
    /// Выход микросхемы (ножка справа); внутри схемы — как выход.
    ChipOut,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Source,
    Processing,
    Output,
    Button,
}

impl NodeKind {
    pub const ALL: [NodeKind; 12] = [
        NodeKind::AppSource,
        NodeKind::MicInput,
        NodeKind::SoundButton,
        NodeKind::Volume,
        NodeKind::Compressor,
        NodeKind::VoiceMorpher,
        NodeKind::Denoise,
        NodeKind::SpeakerOut,
        NodeKind::VirtualOut,
        NodeKind::Chip,
        NodeKind::ChipIn,
        NodeKind::ChipOut,
    ];

    pub fn has_input(self) -> bool {
        matches!(
            self,
            NodeKind::Volume
                | NodeKind::Compressor
                | NodeKind::VoiceMorpher
                | NodeKind::Denoise
                | NodeKind::SpeakerOut
                | NodeKind::VirtualOut
                | NodeKind::Chip
                | NodeKind::ChipOut
        )
    }

    pub fn has_output(self) -> bool {
        !matches!(self, NodeKind::SpeakerOut | NodeKind::VirtualOut | NodeKind::ChipOut)
    }

    pub fn category(self) -> Category {
        match self {
            NodeKind::AppSource | NodeKind::MicInput | NodeKind::ChipIn => Category::Source,
            NodeKind::SoundButton => Category::Button,
            NodeKind::Volume | NodeKind::Compressor | NodeKind::VoiceMorpher | NodeKind::Denoise | NodeKind::Chip => Category::Processing,
            NodeKind::SpeakerOut | NodeKind::VirtualOut | NodeKind::ChipOut => Category::Output,
        }
    }
}

/// Тип порта. Сейчас только «звук»; «управление» заложено в модель, но в интерфейсе не показывается.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PortKind {
    #[default]
    Audio,
    Control,
}

impl PortKind {
    fn is_audio(&self) -> bool {
        *self == PortKind::Audio
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "PortKind::is_audio")]
    pub kind: PortKind,
    /// Ножка микросхемы `from`: id её узла «Выход микросхемы». У обычных узлов — нет.
    #[serde(rename = "fromPort", default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    /// Ножка микросхемы `to`: id её узла «Вход микросхемы».
    #[serde(rename = "toPort", default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

impl Edge {
    /// Обычный звуковой провод без ножек.
    pub fn audio(from: impl Into<String>, to: impl Into<String>) -> Edge {
        Edge { from: from.into(), to: to.into(), kind: PortKind::Audio, from_port: None, to_port: None }
    }
}

/// Схема внутри микросхемы (`params.inner`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Inner {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

/// Схема внутри микросхемы; None — испорчена.
pub fn chip_inner(n: &Node) -> Option<Inner> {
    match n.params.get("inner") {
        None => Some(Inner::default()),
        Some(v) => serde_json::from_value(v.clone()).ok(),
    }
}

/// Ошибка проверки графа. `code()` — ключ для локализованного сообщения в интерфейсе.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphError {
    NewerSchema(u32),
    DuplicateNode(String),
    EmptyId,
    UnknownNode(String),
    NoOutput(String),
    NoInput(String),
    SelfLoop(String),
    DuplicateEdge(String, String),
    Cycle,
    BadParam { node: String, param: &'static str },
    /// Провод к микросхеме без ножки, к несуществующей ножке или ножка у обычного узла.
    BadPort(String),
    TooDeep,
}

impl GraphError {
    pub fn code(&self) -> &'static str {
        match self {
            GraphError::NewerSchema(_) => "newer-schema",
            GraphError::DuplicateNode(_) => "duplicate-node",
            GraphError::EmptyId => "empty-id",
            GraphError::UnknownNode(_) => "unknown-node",
            GraphError::NoOutput(_) => "no-output",
            GraphError::NoInput(_) => "no-input",
            GraphError::SelfLoop(_) => "self-loop",
            GraphError::DuplicateEdge(..) => "duplicate-edge",
            GraphError::Cycle => "cycle",
            GraphError::BadParam { .. } => "bad-param",
            GraphError::BadPort(_) => "bad-port",
            GraphError::TooDeep => "too-deep",
        }
    }
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::NewerSchema(v) => write!(f, "project schema {v} is newer than supported {SCHEMA}"),
            GraphError::DuplicateNode(id) => write!(f, "duplicate node id {id}"),
            GraphError::EmptyId => write!(f, "empty node id"),
            GraphError::UnknownNode(id) => write!(f, "edge refers to unknown node {id}"),
            GraphError::NoOutput(id) => write!(f, "node {id} has no output"),
            GraphError::NoInput(id) => write!(f, "node {id} has no input"),
            GraphError::SelfLoop(id) => write!(f, "node {id} is connected to itself"),
            GraphError::DuplicateEdge(a, b) => write!(f, "duplicate edge {a} -> {b}"),
            GraphError::Cycle => write!(f, "graph has a cycle"),
            GraphError::BadParam { node, param } => write!(f, "node {node}: bad parameter {param}"),
            GraphError::BadPort(id) => write!(f, "edge to node {id} has a wrong chip pin"),
            GraphError::TooDeep => write!(f, "chips are nested deeper than {MAX_CHIP_DEPTH}"),
        }
    }
}

impl std::error::Error for GraphError {}

#[derive(Debug)]
pub enum LoadError {
    Json(serde_json::Error),
    Graph(GraphError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Json(e) => write!(f, "invalid project file: {e}"),
            LoadError::Graph(e) => write!(f, "invalid project: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl LoadError {
    pub fn code(&self) -> &'static str {
        match self {
            LoadError::Json(_) => "bad-json",
            LoadError::Graph(e) => e.code(),
        }
    }
}

/// Проверяет весь граф: идентификаторы, провода, параметры, микросхемы (и всё внутри них),
/// отсутствие циклов (по распакованной схеме — звук через ножки тоже не должен замкнуться).
pub fn validate(p: &Project) -> Result<(), GraphError> {
    if p.schema > SCHEMA {
        return Err(GraphError::NewerSchema(p.schema));
    }
    validate_level(&p.nodes, &p.edges, 0)?;
    topo_order(&flatten(p)).map(|_| ())
}

fn validate_level(nodes: &[Node], edges: &[Edge], depth: usize) -> Result<(), GraphError> {
    if depth > MAX_CHIP_DEPTH {
        return Err(GraphError::TooDeep);
    }
    let mut kinds = HashMap::new();
    // Ножки микросхем: id микросхемы → (входы, выходы).
    let mut pins: HashMap<&str, (HashSet<String>, HashSet<String>)> = HashMap::new();
    for n in nodes {
        if n.id.is_empty() {
            return Err(GraphError::EmptyId);
        }
        if kinds.insert(n.id.as_str(), n.kind).is_some() {
            return Err(GraphError::DuplicateNode(n.id.clone()));
        }
        check_params(n)?;
        if n.kind == NodeKind::Chip {
            let inner = chip_inner(n).ok_or_else(|| GraphError::BadParam { node: n.id.clone(), param: "inner" })?;
            validate_level(&inner.nodes, &inner.edges, depth + 1)?;
            let ids = |k| inner.nodes.iter().filter(|x| x.kind == k).map(|x| x.id.clone()).collect();
            pins.insert(n.id.as_str(), (ids(NodeKind::ChipIn), ids(NodeKind::ChipOut)));
        }
    }
    let mut seen = HashSet::new();
    for e in edges {
        let from = *kinds.get(e.from.as_str()).ok_or_else(|| GraphError::UnknownNode(e.from.clone()))?;
        let to = *kinds.get(e.to.as_str()).ok_or_else(|| GraphError::UnknownNode(e.to.clone()))?;
        if e.from == e.to {
            return Err(GraphError::SelfLoop(e.from.clone()));
        }
        if !from.has_output() {
            return Err(GraphError::NoOutput(e.from.clone()));
        }
        if !to.has_input() {
            return Err(GraphError::NoInput(e.to.clone()));
        }
        // Ножка — только у микросхемы и только существующая.
        let pin_ok = |id: &str, kind: NodeKind, port: &Option<String>, outs: bool| match (kind, port) {
            (NodeKind::Chip, Some(p)) => pins.get(id).is_some_and(|(i, o)| if outs { o.contains(p) } else { i.contains(p) }),
            (NodeKind::Chip, None) => false,
            (_, port) => port.is_none(),
        };
        if !pin_ok(&e.from, from, &e.from_port, true) {
            return Err(GraphError::BadPort(e.from.clone()));
        }
        if !pin_ok(&e.to, to, &e.to_port, false) {
            return Err(GraphError::BadPort(e.to.clone()));
        }
        if !seen.insert((e.from.as_str(), e.from_port.as_deref(), e.to.as_str(), e.to_port.as_deref())) {
            return Err(GraphError::DuplicateEdge(e.from.clone(), e.to.clone()));
        }
    }
    Ok(())
}

/// Распаковывает микросхемы в обычную схему — её и получает звуковой движок. Узлы внутри
/// микросхемы получают id «микросхема/узел» (вложенные — «a/b/узел»); ножки становятся
/// громкостью 1 (звук проходит как есть), провода к микросхеме идут к её ножкам.
/// Схема без микросхем возвращается как есть.
pub fn flatten(p: &Project) -> Project {
    let mut out = Project { schema: p.schema, nodes: Vec::new(), edges: Vec::new() };
    expand(&p.nodes, &p.edges, "", 0, &mut out);
    out
}

fn expand(nodes: &[Node], edges: &[Edge], prefix: &str, depth: usize, out: &mut Project) {
    let mut chips = HashSet::new();
    for n in nodes {
        let id = format!("{prefix}{}", n.id);
        match n.kind {
            NodeKind::Chip => {
                chips.insert(n.id.as_str());
                if depth < MAX_CHIP_DEPTH {
                    if let Some(inner) = chip_inner(n) {
                        expand(&inner.nodes, &inner.edges, &format!("{id}/"), depth + 1, out);
                    }
                }
            }
            NodeKind::ChipIn | NodeKind::ChipOut => out.nodes.push(Node {
                id,
                kind: NodeKind::Volume,
                pos: n.pos,
                params: serde_json::json!({ "gain": 1.0, "mute": false }).as_object().cloned().unwrap_or_default(),
            }),
            _ => out.nodes.push(Node { id, ..n.clone() }),
        }
    }
    let end = |node: &str, port: &Option<String>| match port {
        Some(pin) if chips.contains(node) => format!("{prefix}{node}/{pin}"),
        _ => format!("{prefix}{node}"),
    };
    for e in edges {
        out.edges.push(Edge { from: end(&e.from, &e.from_port), to: end(&e.to, &e.to_port), kind: e.kind, from_port: None, to_port: None });
    }
}

/// Допустимые значения параметров компрессора: порог (дБ), сила (во сколько раз), атака и
/// восстановление (мс), усиление после сжатия (дБ).
pub const COMPRESSOR_RANGES: [(&str, std::ops::RangeInclusive<f64>); 5] = [
    ("threshold", -60.0..=0.0),
    ("ratio", 1.0..=20.0),
    ("attack", 0.1..=100.0),
    ("release", 10.0..=2000.0),
    ("makeup", 0.0..=24.0),
];

/// Допустимые значения морфера: сдвиг высоты (полутона) и сила эффектов 0…1 (с дрожанием).
pub const MORPHER_RANGES: [(&str, std::ops::RangeInclusive<f64>); 5] = [
    ("pitch", -12.0..=12.0),
    ("robot", 0.0..=1.0),
    ("radio", 0.0..=1.0),
    ("echo", 0.0..=1.0),
    ("wobble", 0.0..=1.0),
];

/// Шумоподавление: сила 0…1 (смесь очищенного и исходного).
pub const DENOISE_RANGES: [(&str, std::ops::RangeInclusive<f64>); 1] = [("amount", 0.0..=1.0)];

fn check_params(n: &Node) -> Result<(), GraphError> {
    let bad = |param| GraphError::BadParam { node: n.id.clone(), param };
    if n.kind == NodeKind::Volume {
        if let Some(g) = n.params.get("gain") {
            let g = g.as_f64().ok_or_else(|| bad("gain"))?;
            if !(0.0..=2.0).contains(&g) {
                return Err(bad("gain"));
            }
        }
        if let Some(m) = n.params.get("mute") {
            m.as_bool().ok_or_else(|| bad("mute"))?;
        }
    }
    let ranges: &[(&'static str, std::ops::RangeInclusive<f64>)] = match n.kind {
        NodeKind::Compressor => &COMPRESSOR_RANGES,
        NodeKind::VoiceMorpher => &MORPHER_RANGES,
        NodeKind::Denoise => &DENOISE_RANGES,
        _ => &[],
    };
    for &(param, ref range) in ranges {
        if let Some(v) = n.params.get(param) {
            let v = v.as_f64().ok_or_else(|| bad(param))?;
            if !range.contains(&v) {
                return Err(bad(param));
            }
        }
    }
    Ok(())
}

/// Порядок обработки узлов (источники раньше получателей). Ошибка, если есть цикл.
pub fn topo_order(p: &Project) -> Result<Vec<&str>, GraphError> {
    let mut indegree: HashMap<&str, usize> = p.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut next: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &p.edges {
        if let Some(d) = indegree.get_mut(e.to.as_str()) {
            *d += 1;
        }
        next.entry(e.from.as_str()).or_default().push(e.to.as_str());
    }
    // Узлы в порядке файла — результат стабильный.
    let mut ready: Vec<&str> = p.nodes.iter().map(|n| n.id.as_str()).filter(|id| indegree[id] == 0).collect();
    ready.reverse();
    let mut order = Vec::with_capacity(p.nodes.len());
    while let Some(id) = ready.pop() {
        order.push(id);
        for &to in next.get(id).map(Vec::as_slice).unwrap_or(&[]) {
            let d = indegree.get_mut(to).expect("checked node");
            *d -= 1;
            if *d == 0 {
                ready.push(to);
            }
        }
    }
    if order.len() == p.nodes.len() {
        Ok(order)
    } else {
        Err(GraphError::Cycle)
    }
}

/// Создаст ли провод `from → to` цикл (есть ли уже путь `to → … → from`).
pub fn would_create_cycle(p: &Project, from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    let mut stack = vec![to];
    let mut seen = HashSet::new();
    while let Some(id) = stack.pop() {
        if id == from {
            return true;
        }
        if seen.insert(id) {
            stack.extend(p.edges.iter().filter(|e| e.from == id).map(|e| e.to.as_str()));
        }
    }
    false
}

pub fn from_json(text: &str) -> Result<Project, LoadError> {
    let p: Project = serde_json::from_str(text).map_err(LoadError::Json)?;
    validate(&p).map_err(LoadError::Graph)?;
    Ok(p)
}

pub fn to_json(p: &Project) -> String {
    serde_json::to_string_pretty(p).expect("project is always serializable")
}

/// Проект для первого запуска: приложение → громкость → виртуальный микрофон, плюс микрофон.
pub fn default_project() -> Project {
    let node = |id: &str, kind, x, y, params: Value| Node {
        id: id.into(),
        kind,
        pos: [x, y],
        params: params.as_object().cloned().unwrap_or_default(),
    };
    let edge = |from: &str, to: &str| Edge::audio(from, to);
    Project {
        schema: SCHEMA_BASE,
        nodes: vec![
            node("n1", NodeKind::AppSource, 0.0, 0.0, serde_json::json!({ "exe": "" })),
            node("n2", NodeKind::MicInput, 0.0, 160.0, serde_json::json!({ "device": "default" })),
            node("n3", NodeKind::Volume, 260.0, 0.0, serde_json::json!({ "gain": 1.0, "mute": false })),
            node("n4", NodeKind::VirtualOut, 520.0, 60.0, serde_json::json!({ "device": "virtual-1" })),
        ],
        edges: vec![edge("n1", "n3"), edge("n2", "n4"), edge("n3", "n4")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCH_EXAMPLE: &str = r#"{
      "schema": 1,
      "nodes": [
        {"id": "n1", "type": "app-source",   "pos": [0, 0],    "params": {"exe": "discord.exe"}},
        {"id": "n2", "type": "mic-input",    "pos": [0, 160],  "params": {"device": "default"}},
        {"id": "n3", "type": "volume",       "pos": [260, 0],  "params": {"gain": 1.0, "mute": false}},
        {"id": "n4", "type": "virtual-out",  "pos": [520, 60], "params": {"device": "virtual-1"}}
      ],
      "edges": [
        {"from": "n1", "to": "n3"},
        {"from": "n2", "to": "n4"},
        {"from": "n3", "to": "n4"}
      ]
    }"#;

    fn edge(from: &str, to: &str) -> Edge {
        Edge::audio(from, to)
    }

    #[test]
    fn reads_architecture_example() {
        let p = from_json(ARCH_EXAMPLE).unwrap();
        assert_eq!(p.nodes.len(), 4);
        assert_eq!(p.nodes[0].kind, NodeKind::AppSource);
        assert_eq!(p.nodes[0].params["exe"], "discord.exe");
    }

    #[test]
    fn json_roundtrip_keeps_project() {
        let p = default_project();
        let back = from_json(&to_json(&p)).unwrap();
        assert_eq!(p, back);
        assert!(!to_json(&p).contains("\"kind\""), "audio edges are written without kind");
    }

    #[test]
    fn default_project_is_valid() {
        validate(&default_project()).unwrap();
    }

    #[test]
    fn topo_order_puts_sources_first() {
        let p = default_project();
        let order = topo_order(&p).unwrap();
        let at = |id| order.iter().position(|x| *x == id).unwrap();
        assert!(at("n1") < at("n3") && at("n3") < at("n4") && at("n2") < at("n4"));
    }

    #[test]
    fn rejects_cycle() {
        let mut p = default_project();
        p.nodes.push(Node { id: "v2".into(), kind: NodeKind::Volume, pos: [0.0, 0.0], params: Map::new() });
        p.edges.push(edge("n3", "v2"));
        p.edges.push(edge("v2", "n3"));
        assert_eq!(validate(&p), Err(GraphError::Cycle));
    }

    #[test]
    fn detects_future_cycle() {
        let mut p = default_project();
        p.nodes.push(Node { id: "v2".into(), kind: NodeKind::Volume, pos: [0.0, 0.0], params: Map::new() });
        p.edges.push(edge("n3", "v2"));
        assert!(would_create_cycle(&p, "v2", "n3"));
        assert!(!would_create_cycle(&p, "n1", "v2"));
        assert!(would_create_cycle(&p, "n3", "n3"));
    }

    #[test]
    fn rejects_wrong_directions_and_duplicates() {
        let mut p = default_project();
        p.edges.push(edge("n4", "n3"));
        assert_eq!(validate(&p), Err(GraphError::NoOutput("n4".into())));

        let mut p = default_project();
        p.edges.push(edge("n3", "n1"));
        assert_eq!(validate(&p), Err(GraphError::NoInput("n1".into())));

        let mut p = default_project();
        p.edges.push(edge("n1", "n3"));
        assert_eq!(validate(&p), Err(GraphError::DuplicateEdge("n1".into(), "n3".into())));

        let mut p = default_project();
        p.edges.push(edge("n1", "zz"));
        assert_eq!(validate(&p), Err(GraphError::UnknownNode("zz".into())));

        let mut p = default_project();
        p.nodes[1].id = "n1".into();
        assert_eq!(validate(&p), Err(GraphError::DuplicateNode("n1".into())));
    }

    #[test]
    fn several_wires_into_one_input_are_allowed() {
        // Вход принимает несколько проводов (звук складывается), выход идёт в несколько входов.
        let mut p = default_project();
        p.nodes.push(Node { id: "o2".into(), kind: NodeKind::SpeakerOut, pos: [0.0, 0.0], params: Map::new() });
        p.edges.push(edge("n3", "o2"));
        p.edges.push(edge("n2", "o2"));
        validate(&p).unwrap();
    }

    #[test]
    fn checks_volume_params() {
        let mut p = default_project();
        p.nodes[2].params.insert("gain".into(), serde_json::json!(2.5));
        assert_eq!(validate(&p).unwrap_err().code(), "bad-param");
        p.nodes[2].params.insert("gain".into(), serde_json::json!("loud"));
        assert_eq!(validate(&p).unwrap_err().code(), "bad-param");
    }

    #[test]
    fn checks_compressor_params() {
        let mut p = default_project();
        p.nodes.push(Node { id: "c".into(), kind: NodeKind::Compressor, pos: [0.0, 0.0], params: Map::new() });
        p.edges.push(edge("n1", "c"));
        p.edges.push(edge("c", "n4"));
        validate(&p).unwrap();
        p.nodes.last_mut().unwrap().params.insert("ratio".into(), serde_json::json!(0.5));
        assert_eq!(validate(&p).unwrap_err().code(), "bad-param");
        p.nodes.last_mut().unwrap().params.insert("ratio".into(), serde_json::json!(4));
        p.nodes.last_mut().unwrap().params.insert("threshold".into(), serde_json::json!(6));
        assert_eq!(validate(&p).unwrap_err().code(), "bad-param");
        let text = to_json(&p);
        assert!(text.contains("\"compressor\""));
    }

    #[test]
    fn rejects_newer_schema_and_bad_json() {
        let newer = ARCH_EXAMPLE.replace("\"schema\": 1", "\"schema\": 99");
        assert_eq!(from_json(&newer).unwrap_err().code(), "newer-schema");
        assert_eq!(from_json("{ not json").unwrap_err().code(), "bad-json");
        let unknown = ARCH_EXAMPLE.replace("app-source", "teleport");
        assert_eq!(from_json(&unknown).unwrap_err().code(), "bad-json");
    }

    #[test]
    fn control_port_kind_survives_roundtrip() {
        let mut p = default_project();
        p.edges[0].kind = PortKind::Control;
        let text = to_json(&p);
        assert!(text.contains("\"kind\": \"control\""));
        assert_eq!(from_json(&text).unwrap().edges[0].kind, PortKind::Control);
    }

    /// Микросхема «Голос»: вход → громкость → выход; снаружи микрофон → микросхема → кабель.
    const CHIP_EXAMPLE: &str = r#"{
      "schema": 2,
      "nodes": [
        {"id": "m", "type": "mic-input", "pos": [0, 0], "params": {"device": "default"}},
        {"id": "c", "type": "chip", "pos": [300, 0], "params": {"name": "Голос", "inner": {
          "nodes": [
            {"id": "i", "type": "chip-in",  "pos": [0, 0],   "params": {"name": "Голос"}},
            {"id": "v", "type": "volume",   "pos": [200, 0], "params": {"gain": 0.5, "mute": false}},
            {"id": "o", "type": "chip-out", "pos": [400, 0], "params": {"name": "Выход"}}
          ],
          "edges": [{"from": "i", "to": "v"}, {"from": "v", "to": "o"}]
        }}},
        {"id": "k", "type": "virtual-out", "pos": [600, 0], "params": {"device": "virtual-1"}}
      ],
      "edges": [
        {"from": "m", "to": "c", "toPort": "i"},
        {"from": "c", "fromPort": "o", "to": "k"}
      ]
    }"#;

    #[test]
    fn old_files_stay_byte_for_byte_the_same() {
        // Файл 0.4 читается и пишется ровно как раньше: без новых полей, версия 1.
        let p = from_json(ARCH_EXAMPLE).unwrap();
        let text = to_json(&p);
        assert!(text.contains("\"schema\": 1"));
        assert!(!text.contains("Port"));
        assert_eq!(from_json(&text).unwrap(), p);
        assert_eq!(flatten(&p), p, "без микросхем распаковка ничего не меняет");
        assert_eq!(default_project().schema, SCHEMA_BASE);
    }

    #[test]
    fn chip_validates_and_flattens_for_the_engine() {
        let p = from_json(CHIP_EXAMPLE).unwrap();
        assert_eq!(from_json(&to_json(&p)).unwrap(), p, "микросхема переживает сохранение");
        let flat = flatten(&p);
        let ids: Vec<_> = flat.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["m", "c/i", "c/v", "c/o", "k"]);
        // Ножки — громкость 1 (звук проходит как есть), громкость внутри — своя.
        assert_eq!(flat.nodes[1].kind, NodeKind::Volume);
        assert_eq!(flat.nodes[1].params["gain"], 1.0);
        assert_eq!(flat.nodes[2].params["gain"], 0.5);
        let edges: Vec<_> = flat.edges.iter().map(|e| (e.from.as_str(), e.to.as_str())).collect();
        assert_eq!(edges, [("c/i", "c/v"), ("c/v", "c/o"), ("m", "c/i"), ("c/o", "k")]);
        assert!(flat.edges.iter().all(|e| e.from_port.is_none() && e.to_port.is_none()));
        validate(&flat).unwrap();
    }

    #[test]
    fn chip_in_chip_flattens_with_nested_ids() {
        let mut p = from_json(CHIP_EXAMPLE).unwrap();
        // Внешнюю микросхему кладём внутрь новой, с такими же ножками.
        let inner_chip = p.nodes[1].clone();
        let outer = serde_json::json!({
            "nodes": [
                {"id": "i", "type": "chip-in", "pos": [0, 0], "params": {}},
                inner_chip,
                {"id": "o", "type": "chip-out", "pos": [0, 0], "params": {}}
            ],
            "edges": [{"from": "i", "to": "c", "toPort": "i"}, {"from": "c", "fromPort": "o", "to": "o"}]
        });
        p.nodes[1].params.insert("inner".into(), outer);
        validate(&p).unwrap();
        let ids: Vec<_> = flatten(&p).nodes.into_iter().map(|n| n.id).collect();
        assert_eq!(ids, ["m", "c/i", "c/c/i", "c/c/v", "c/c/o", "c/o", "k"]);
    }

    #[test]
    fn rejects_wrong_chip_pins_and_loops_through_chips() {
        let ok = from_json(CHIP_EXAMPLE).unwrap();
        // Провод к микросхеме без ножки, к чужой ножке, ножка у обычного узла.
        let mut p = ok.clone();
        p.edges[0].to_port = None;
        assert_eq!(validate(&p).unwrap_err().code(), "bad-port");
        p.edges[0].to_port = Some("o".into());
        assert_eq!(validate(&p).unwrap_err().code(), "bad-port");
        let mut p = ok.clone();
        p.edges[1].to_port = Some("x".into());
        assert_eq!(validate(&p).unwrap_err().code(), "bad-port");
        // Испорченная схема внутри.
        let mut p = ok.clone();
        p.nodes[1].params.insert("inner".into(), serde_json::json!("мусор"));
        assert_eq!(validate(&p).unwrap_err().code(), "bad-param");
        // Ошибка внутри микросхемы тоже ловится.
        let mut p = ok.clone();
        let mut inner = chip_inner(&p.nodes[1]).unwrap();
        inner.nodes[1].params.insert("gain".into(), serde_json::json!(9));
        p.nodes[1].params.insert("inner".into(), serde_json::to_value(inner).unwrap());
        assert_eq!(validate(&p).unwrap_err().code(), "bad-param");
        // Звук по кругу через ножки: выход микросхемы → громкость → снова в её вход.
        let mut p = ok.clone();
        p.nodes.push(Node { id: "g".into(), kind: NodeKind::Volume, pos: [0.0, 0.0], params: Map::new() });
        p.edges.push(Edge { from_port: Some("o".into()), ..Edge::audio("c", "g") });
        p.edges.push(Edge { to_port: Some("i".into()), ..Edge::audio("g", "c") });
        assert_eq!(validate(&p).unwrap_err().code(), "cycle");
        // Файл 0.5 с микросхемой — новее, чем понимала 0.4 (она отложит его, а не испортит).
        assert_eq!(ok.schema, 2);
    }
}
