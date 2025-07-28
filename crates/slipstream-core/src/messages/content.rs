use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeStruct as _};
use serde_json::Value;

use ::base64::{Engine as _, engine::general_purpose};

/* --------------------------------------------------------------------- */
/*  ContentOrParts                                                       */
/* --------------------------------------------------------------------- */

#[derive(Clone, Debug, PartialEq)]
pub enum ContentOrParts {
  Content(String),
  Parts(Vec<ContentPart>),
}

impl Default for ContentOrParts {
  fn default() -> Self {
    ContentOrParts::Content(String::new())
  }
}

impl Serialize for ContentOrParts {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    match self {
      ContentOrParts::Content(s) if !s.trim().is_empty() => serializer.serialize_str(s),
      ContentOrParts::Parts(parts) => parts.serialize(serializer),
      _ => serializer.serialize_none(),
    }
  }
}

impl<'de> Deserialize<'de> for ContentOrParts {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let v: Value = Deserialize::deserialize(deserializer)?;
    Ok(match v {
      Value::String(s) => ContentOrParts::Content(s),
      Value::Array(arr) => {
        let parts: Vec<ContentPart> =
          Vec::<ContentPart>::deserialize(Value::Array(arr)).map_err(de::Error::custom)?;
        ContentOrParts::Parts(parts)
      }
      Value::Null => ContentOrParts::Content(String::new()),
      _ => {
        return Err(de::Error::custom(
          "ContentOrParts expects string, array or null",
        ));
      }
    })
  }
}

/* --------------------------------------------------------------------- */
/*  AssistantContentOrParts                                              */
/* --------------------------------------------------------------------- */

#[derive(Clone, Debug, PartialEq)]
pub enum AssistantContentOrParts {
  Content(String),
  Refusal(String),
  Parts(Vec<AssistantContentPart>),
}

impl Default for AssistantContentOrParts {
  fn default() -> Self {
    AssistantContentOrParts::Content(String::new())
  }
}

impl Serialize for AssistantContentOrParts {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    match self {
      AssistantContentOrParts::Content(s) if !s.trim().is_empty() => serializer.serialize_str(s),
      AssistantContentOrParts::Refusal(s) if !s.trim().is_empty() => serializer.serialize_str(s),
      AssistantContentOrParts::Parts(parts) => parts.serialize(serializer),

      _ => serializer.serialize_none(),
    }
  }
}

impl<'de> Deserialize<'de> for AssistantContentOrParts {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let v: Value = Deserialize::deserialize(deserializer)?;
    Ok(match v {
      Value::String(s) => {
        // We can't distinguish between content and refusal from just a string,
        // so we default to content. The refusal should be set through the proper variant.
        AssistantContentOrParts::Content(s)
      }
      Value::Array(arr) => {
        let parts: Vec<AssistantContentPart> =
          Vec::<AssistantContentPart>::deserialize(Value::Array(arr)).map_err(de::Error::custom)?;
        AssistantContentOrParts::Parts(parts)
      }
      Value::Null => AssistantContentOrParts::Content(String::new()),
      _ => {
        return Err(de::Error::custom(
          "AssistantContentOrParts expects string, array or null",
        ));
      }
    })
  }
}

/* --------------------------------------------------------------------- */
/*  Content & Assistant parts                                            */
/* --------------------------------------------------------------------- */

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentPart {
  Text(TextContentPart),
  Image(ImageContentPart),
  Audio(AudioContentPart),
  Video(VideoContentPart),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AssistantContentPart {
  Text(TextContentPart),
  Refusal(RefusalContentPart),
}

/* --------------------------------------------------------------------- */
/*  Text & Refusal                                                       */
/* --------------------------------------------------------------------- */

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextContentPart {
  pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RefusalContentPart {
  pub refusal: String,
}

/* --------------------------------------------------------------------- */
/*  Image                                                                */
/* --------------------------------------------------------------------- */

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageContentPart {
  #[serde(rename = "image_url")]
  pub url: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub detail: Option<String>,
}

/* --------------------------------------------------------------------- */
/*  Audio / Video helper structs                                         */
/* --------------------------------------------------------------------- */

#[derive(Clone, Debug, PartialEq)]
pub struct InputAudio {
  pub data: Vec<u8>,
  pub format: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputVideo {
  pub data: Vec<u8>,
  pub format: String,
}

impl Serialize for InputAudio {
  fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    let mut st = s.serialize_struct("InputAudio", 2)?;
    st.serialize_field("data", &general_purpose::STANDARD.encode(&self.data))?;
    st.serialize_field("format", &self.format)?;
    st.end()
  }
}
impl<'de> Deserialize<'de> for InputAudio {
  fn deserialize<D>(d: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    #[derive(Deserialize)]
    struct Aux {
      data: String,
      format: String,
    }
    let aux = Aux::deserialize(d)?;
    let decoded = general_purpose::STANDARD
      .decode(&aux.data)
      .map_err(de::Error::custom)?;
    Ok(Self {
      data: decoded,
      format: aux.format,
    })
  }
}

impl Serialize for InputVideo {
  fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    let mut st = s.serialize_struct("InputVideo", 2)?;
    st.serialize_field("data", &general_purpose::STANDARD.encode(&self.data))?;
    st.serialize_field("format", &self.format)?;
    st.end()
  }
}
impl<'de> Deserialize<'de> for InputVideo {
  fn deserialize<D>(d: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    #[derive(Deserialize)]
    struct Aux {
      data: String,
      format: String,
    }
    let aux = Aux::deserialize(d)?;
    let decoded = general_purpose::STANDARD
      .decode(&aux.data)
      .map_err(de::Error::custom)?;
    Ok(Self {
      data: decoded,
      format: aux.format,
    })
  }
}

/* --------------------------------------------------------------------- */
/*  Audio / Video parts                                                  */
/* --------------------------------------------------------------------- */

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioContentPart {
  pub input_audio: InputAudio,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoContentPart {
  pub input_video: InputVideo,
}

/* --------------------------------------------------------------------- */
/*  Convenience constructors                                             */
/* --------------------------------------------------------------------- */

pub fn text<T: Into<String>>(t: T) -> TextContentPart {
  TextContentPart { text: t.into() }
}
pub fn refusal<T: Into<String>>(t: T) -> RefusalContentPart {
  RefusalContentPart { refusal: t.into() }
}
pub fn image<T: Into<String>>(url: T) -> ImageContentPart {
  ImageContentPart {
    url: url.into(),
    detail: None,
  }
}
pub fn audio(data: Vec<u8>, format: &str) -> ContentPart {
  ContentPart::Audio(AudioContentPart {
    input_audio: InputAudio {
      data,
      format: format.to_string(),
    },
  })
}
pub fn video(data: Vec<u8>, format: &str) -> ContentPart {
  ContentPart::Video(VideoContentPart {
    input_video: InputVideo {
      data,
      format: format.to_string(),
    },
  })
}

// ...existing code...

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::Value;
  use std::{iter, sync::Arc, thread};

  /* -------------------------------------------------- */
  /* helpers                                            */
  /* -------------------------------------------------- */

  fn json_eq(a: &str, b: &str) {
    let va: Value = serde_json::from_str(a).unwrap();
    let vb: Value = serde_json::from_str(b).unwrap();
    assert_eq!(va, vb);
  }
  fn assert_roundtrip<T>(value: &T)
  where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + core::fmt::Debug,
  {
    let enc = serde_json::to_vec(value).unwrap();
    let dec: T = serde_json::from_slice(&enc).unwrap();
    assert_eq!(&dec, value);
  }

  /* -------------------------------------------------- */
  /* ContentOrParts – marshal / unmarshal               */
  /* -------------------------------------------------- */

  #[test]
  fn content_or_parts_marshal() {
    let cases = &[
      ("null when empty", ContentOrParts::default(), "null"),
      (
        "plain string",
        ContentOrParts::Content("hello world".into()),
        r#""hello world""#,
      ),
      (
        "whitespace string treated as null",
        ContentOrParts::Content("   ".into()),
        "null",
      ),
      (
        "single text part",
        ContentOrParts::Parts(vec![ContentPart::Text(text("hello world"))]),
        r#"[{"type":"text","text":"hello world"}]"#,
      ),
      (
        "single image part",
        ContentOrParts::Parts(vec![ContentPart::Image(image(
          "http://example.com/image.jpg",
        ))]),
        r#"[{"type":"image","image_url":"http://example.com/image.jpg"}]"#,
      ),
      (
        "single audio",
        ContentOrParts::Parts(vec![audio(b"test audio data".to_vec(), "mp3")]),
        r#"[{"type":"audio","input_audio":{"data":"dGVzdCBhdWRpbyBkYXRh","format":"mp3"}}]"#,
      ),
      (
        "single video",
        ContentOrParts::Parts(vec![video(b"test video data".to_vec(), "mp4")]),
        r#"[{"type":"video","input_video":{"data":"dGVzdCB2aWRlbyBkYXRh","format":"mp4"}}]"#,
      ),
      (
        "mixed parts",
        ContentOrParts::Parts(vec![
          ContentPart::Text(text("hello")),
          ContentPart::Image(image("http://example.com/image.jpg")),
          audio(b"test audio data".to_vec(), "mp3"),
          video(b"test video data".to_vec(), "mp4"),
        ]),
        r#"[{"type":"text","text":"hello"},{"type":"image","image_url":"http://example.com/image.jpg"},{"type":"audio","input_audio":{"data":"dGVzdCBhdWRpbyBkYXRh","format":"mp3"}},{"type":"video","input_video":{"data":"dGVzdCB2aWRlbyBkYXRh","format":"mp4"}}]"#,
      ),
    ];

    for (name, in_val, want) in cases {
      let got = serde_json::to_string(in_val).unwrap();
      json_eq(&got, want);
      // round-trip
      if name == &"whitespace string treated as null" {
        // Special case: whitespace serializes to null, deserializes to None
        let dec: ContentOrParts = serde_json::from_str(&got).unwrap();
        assert_eq!(dec, ContentOrParts::default());
      } else {
        assert_roundtrip(in_val);
      }
    }
  }

  #[test]
  fn content_or_parts_unmarshal_invalid() {
    assert!(serde_json::from_str::<ContentOrParts>("{invalid json").is_err());
  }

  #[test]
  fn content_or_parts_unmarshal_various() {
    let cases = &[
      (r#"[]"#, ContentOrParts::Parts(vec![])),
      (
        r#""hello world""#,
        ContentOrParts::Content("hello world".into()),
      ),
      (
        r#"[{"type":"text","text":"hello world"}]"#,
        ContentOrParts::Parts(vec![ContentPart::Text(text("hello world"))]),
      ),
    ];
    for (raw, want) in cases {
      let got: ContentOrParts = serde_json::from_str(raw).unwrap();
      assert_eq!(&got, want);
    }
  }

  /* -------------------------------------------------- */
  /* AssistantContentOrParts                            */
  /* -------------------------------------------------- */

  #[test]
  fn assistant_content_or_parts_marshal() {
    let cases = &[
      (
        "null when empty",
        AssistantContentOrParts::default(),
        "null",
      ),
      (
        "plain string",
        AssistantContentOrParts::Content("hello world".into()),
        r#""hello world""#,
      ),
      (
        "empty string treated as null",
        AssistantContentOrParts::Content("".into()),
        "null",
      ),
      (
        "text part",
        AssistantContentOrParts::Parts(vec![AssistantContentPart::Text(text("hello world"))]),
        r#"[{"type":"text","text":"hello world"}]"#,
      ),
      (
        "refusal part",
        AssistantContentOrParts::Parts(vec![AssistantContentPart::Refusal(refusal(
          "I cannot help with that",
        ))]),
        r#"[{"type":"refusal","refusal":"I cannot help with that"}]"#,
      ),
    ];
    for (name, in_val, want) in cases {
      let got = serde_json::to_string(in_val).unwrap();

      json_eq(&got, want);
      assert_roundtrip(in_val);
    }
  }

  /* -------------------------------------------------- */
  /* Individual part structs                            */
  /* -------------------------------------------------- */

  #[test]
  fn roundtrip_parts() {
    let parts: &[ContentPart] = &[
      ContentPart::Text(text("abc")),
      ContentPart::Image(image("http://x/y.jpg")),
      audio(b"zzz".to_vec(), "wav"),
      video(b"vvv".to_vec(), "mp4"),
    ];
    for p in parts {
      assert_roundtrip(p);
    }
    let a_parts: &[AssistantContentPart] = &[
      AssistantContentPart::Text(text("abc")),
      AssistantContentPart::Refusal(refusal("no")),
    ];
    for p in a_parts {
      assert_roundtrip(p);
    }
  }

  /* -------------------------------------------------- */
  /* Edge-cases                                         */
  /* -------------------------------------------------- */

  #[test]
  fn very_large_content() {
    let big = "a".repeat(1 << 20); // 1 MiB
    let cop = ContentOrParts::Content(big.clone());
    assert_roundtrip(&cop);
    match &cop {
      ContentOrParts::Content(s) => assert_eq!(s.len(), big.len()),
      _ => panic!("Expected Content variant"),
    }
  }

  #[test]
  fn empty_parts_vs_null() {
    let empty = ContentOrParts::Parts(vec![]);
    assert_eq!(serde_json::to_string(&empty).unwrap(), "[]");

    let nil = ContentOrParts::default();
    assert_eq!(serde_json::to_string(&nil).unwrap(), "null");
  }

  #[test]
  fn concurrent_marshal() {
    let val = Arc::new(ContentOrParts::Parts(vec![
      ContentPart::Text(text("hello")),
      ContentPart::Image(image("http://example.com")),
    ]));
    let mut handles = vec![];
    for _ in 0..10 {
      let v = Arc::clone(&val);
      handles.push(thread::spawn(move || {
        serde_json::to_vec(&*v).unwrap();
      }));
    }
    for h in handles {
      h.join().unwrap();
    }
  }

  /* -------------------------------------------------- */
  /* Special-character handling                         */
  /* -------------------------------------------------- */

  #[test]
  fn special_characters() {
    let txt = TextContentPart {
      text: "Hello\n\t\"'\\世界\u{0}".into(),
    };
    let raw = serde_json::to_string(&ContentPart::Text(txt.clone())).unwrap();
    json_eq(&raw, r#"{"type":"text","text":"Hello\n\t\"'\\世界\u0000"}"#);
    assert_roundtrip(&txt);

    let refu = RefusalContentPart {
      refusal: "Cannot<process>HTML&tags\n\r\t".into(),
    };
    let raw = serde_json::to_string(&AssistantContentPart::Refusal(refu.clone())).unwrap();
    json_eq(
      &raw,
      r#"{"type":"refusal","refusal":"Cannot\u003cprocess\u003eHTML\u0026tags\n\r\t"}"#,
    );
    assert_roundtrip(&refu);

    let img = ImageContentPart {
      url: "https://example.com/image?name=test&size=large#frag".into(),
      detail: None,
    };
    let raw = serde_json::to_string(&ContentPart::Image(img.clone())).unwrap();
    json_eq(
      &raw,
      r#"{"type":"image","image_url":"https://example.com/image?name=test\u0026size=large#frag"}"#,
    );
    assert_roundtrip(&img);
  }
}
