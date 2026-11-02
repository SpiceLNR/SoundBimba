// SPDX-License-Identifier: AGPL-3.0-or-later
//! Модель графа SoundBimba (файл проекта): узлы, провода, проверка, чтение и запись JSON.
//! Узлы не знают о звуке — граф можно открыть и без звуковых устройств.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Текущая версия схемы файла проекта.
pub const SCHEMA: u32 = 1;

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
    pub const ALL: [NodeKind; 9] = [
        NodeKind::AppSource,
        NodeKind::MicInput,
        NodeKind::SoundButton,
        NodeKind::Volume,
        NodeKind::Compressor,
        NodeKind::VoiceMorpher,
        NodeKind::Denoise,
        NodeKind::SpeakerOut,
        NodeKind::VirtualOut,
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
        )
    }

    pub fn has_output(self) -> bool {
        !matches!(self, NodeKind::SpeakerOut | NodeKind::VirtualOut)
    }

    pub fn category(self) -> Category {
        match self {
            NodeKind::AppSource | NodeKind::MicInput => Category::Source,
            NodeKind::SoundButton => Category::Button,
            NodeKind::Volume | NodeKind::Compressor | NodeKind::VoiceMorpher | NodeKind::Denoise => Category::Processing,
            NodeKind::SpeakerOut | NodeKind::VirtualOut => Category::Output,
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

/// Проверяет весь граф: идентификаторы, провода, параметры, отсутствие циклов.
pub fn validate(p: &Project) -> Result<(), GraphError> {
    if p.schema > SCHEMA {
        return Err(GraphError::NewerSchema(p.schema));
    }
    let mut kinds = HashMap::new();
    for n in &p.nodes {
        if n.id.is_empty() {
            return Err(GraphError::EmptyId);
        }
        if kinds.insert(n.id.as_str(), n.kind).is_some() {
            return Err(GraphError::DuplicateNode(n.id.clone()));
        }
        check_params(n)?;
    }
    let mut seen = HashSet::new();
    for e in &p.edges {
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
        if !seen.insert((e.from.as_str(), e.to.as_str())) {
            return Err(GraphError::DuplicateEdge(e.from.clone(), e.to.clone()));
        }
    }
    topo_order(p).map(|_| ())
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
    let edge = |from: &str, to: &str| Edge { from: from.into(), to: to.into(), kind: PortKind::Audio };
    Project {
        schema: SCHEMA,
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
        Edge { from: from.into(), to: to.into(), kind: PortKind::Audio }
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
}
