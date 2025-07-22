//! Rust port of the Go `messages` package.
//!
//!  • `ContentOrParts`   – `"text"` string OR `[parts]` array OR `null`
//!  • `Assistant…`       – same, but allows a plain refusal string
//!  • Tagged `ContentPart` / `AssistantContentPart` enums
//!  • Audio / video data base-64 encoded exactly like the Go version

use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeStruct as _};
use serde_json::Value;

use ::base64::{Engine as _, engine::general_purpose};

/* --------------------------------------------------------------------- */
/*  ContentOrParts                                                       */
/* --------------------------------------------------------------------- */

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentOrParts {
  pub content: Option<String>,
  pub parts: Option<Vec<ContentPart>>,
}

impl Serialize for ContentOrParts {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    match (
      self
        .content
        .as_ref()
        .map_or(false, |f| !f.trim().is_empty()),
      self.parts.as_ref(),
    ) {
      (true, _) => serializer.serialize_str(self.content.as_ref().unwrap()),
      (_, Some(parts)) => parts.serialize(serializer),
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
      Value::String(s) => Self {
        content: Some(s),
        parts: None,
      },
      Value::Array(arr) => {
        let parts: Vec<ContentPart> =
          Vec::<ContentPart>::deserialize(Value::Array(arr)).map_err(de::Error::custom)?;
        Self {
          content: None,
          parts: Some(parts),
        }
      }
      Value::Null => Self::default(),
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssistantContentOrParts {
  pub content: Option<String>,
  pub refusal: Option<String>,
  pub parts: Option<Vec<AssistantContentPart>>,
}

impl Serialize for AssistantContentOrParts {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    if self
      .content
      .as_ref()
      .map(|c| !c.trim().is_empty())
      .unwrap_or(false)
      && self
        .refusal
        .as_ref()
        .map(|c| !c.trim().is_empty())
        .unwrap_or(false)
    {
      return Err(serde::ser::Error::custom(
        "both content and refusal are set",
      ));
    }

    if let Some(ref s) = self.content {
      return serializer.serialize_str(s);
    }
    if let Some(ref s) = self.refusal {
      return serializer.serialize_str(s);
    }
    if let Some(ref parts) = self.parts {
      return parts.serialize(serializer);
    }
    serializer.serialize_none()
  }
}

impl<'de> Deserialize<'de> for AssistantContentOrParts {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let v: Value = Deserialize::deserialize(deserializer)?;
    Ok(match v {
      Value::String(s) => Self {
        content: Some(s),
        refusal: None,
        parts: None,
      },
      Value::Array(arr) => {
        let parts: Vec<AssistantContentPart> =
          Vec::<AssistantContentPart>::deserialize(Value::Array(arr)).map_err(de::Error::custom)?;
        Self {
          content: None,
          refusal: None,
          parts: Some(parts),
        }
      }
      Value::Null => Self::default(),
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
  use serde_json::{Value, json};
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
        ContentOrParts {
          content: Some("hello world".into()),
          parts: None,
        },
        r#""hello world""#,
      ),
      (
        "whitespace string treated as null",
        ContentOrParts {
          content: Some("   ".into()),
          parts: None,
        },
        "null",
      ),
      (
        "single text part",
        ContentOrParts {
          content: None,
          parts: Some(vec![ContentPart::Text(text("hello world"))]),
        },
        r#"[{"type":"text","text":"hello world"}]"#,
      ),
      (
        "single image part",
        ContentOrParts {
          content: None,
          parts: Some(vec![ContentPart::Image(image(
            "http://example.com/image.jpg",
          ))]),
        },
        r#"[{"type":"image","image_url":"http://example.com/image.jpg"}]"#,
      ),
      (
        "single audio",
        ContentOrParts {
          content: None,
          parts: Some(vec![audio(b"test audio data".to_vec(), "mp3")]),
        },
        r#"[{"type":"audio","input_audio":{"data":"dGVzdCBhdWRpbyBkYXRh","format":"mp3"}}]"#,
      ),
      (
        "single video",
        ContentOrParts {
          content: None,
          parts: Some(vec![video(b"test video data".to_vec(), "mp4")]),
        },
        r#"[{"type":"video","input_video":{"data":"dGVzdCB2aWRlbyBkYXRh","format":"mp4"}}]"#,
      ),
      (
        "mixed parts",
        ContentOrParts {
          content: None,
          parts: Some(vec![
            ContentPart::Text(text("hello")),
            ContentPart::Image(image("http://example.com/image.jpg")),
            audio(b"test audio data".to_vec(), "mp3"),
            video(b"test video data".to_vec(), "mp4"),
          ]),
        },
        r#"[{"type":"text","text":"hello"},{"type":"image","image_url":"http://example.com/image.jpg"},{"type":"audio","input_audio":{"data":"dGVzdCBhdWRpbyBkYXRh","format":"mp3"}},{"type":"video","input_video":{"data":"dGVzdCB2aWRlbyBkYXRh","format":"mp4"}}]"#,
      ),
    ];

    for (name, in_val, want) in cases {
      let got = serde_json::to_string(in_val).unwrap();
      json_eq(&got, want);
      // round-trip
      assert_roundtrip(in_val);
      println!("ok – {name}");
    }
  }

  #[test]
  fn content_or_parts_unmarshal_invalid() {
    assert!(serde_json::from_str::<ContentOrParts>("{invalid json").is_err());
  }

  #[test]
  fn content_or_parts_unmarshal_various() {
    let cases = &[
      (
        r#"[]"#,
        ContentOrParts {
          parts: Some(vec![]),
          content: None,
        },
      ),
      (
        r#""hello world""#,
        ContentOrParts {
          content: Some("hello world".into()),
          parts: None,
        },
      ),
      (
        r#"[{"type":"text","text":"hello world"}]"#,
        ContentOrParts {
          parts: Some(vec![ContentPart::Text(text("hello world"))]),
          content: None,
        },
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
      ("null", AssistantContentOrParts::default(), "null"),
      (
        "plain string",
        AssistantContentOrParts {
          content: Some("hello world".into()),
          ..Default::default()
        },
        r#""hello world""#,
      ),
      (
        "text part",
        AssistantContentOrParts {
          parts: Some(vec![AssistantContentPart::Text(text("hello world"))]),
          ..Default::default()
        },
        r#"[{"type":"text","text":"hello world"}]"#,
      ),
      (
        "refusal part",
        AssistantContentOrParts {
          parts: Some(vec![AssistantContentPart::Refusal(refusal(
            "I cannot help with that",
          ))]),
          ..Default::default()
        },
        r#"[{"type":"refusal","refusal":"I cannot help with that"}]"#,
      ),
    ];
    for (name, in_val, want) in cases {
      let got = serde_json::to_string(in_val).unwrap();
      json_eq(&got, want);
      assert_roundtrip(in_val);
      println!("ok – {name}");
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
    let big = iter::repeat('a').take(1 << 20).collect::<String>(); // 1 MiB
    let cop = ContentOrParts {
      content: Some(big.clone()),
      parts: None,
    };
    assert_roundtrip(&cop);
    assert_eq!(cop.content.as_ref().unwrap().len(), big.len());
  }

  #[test]
  fn empty_parts_vs_null() {
    let empty = ContentOrParts {
      parts: Some(vec![]),
      content: None,
    };
    assert_eq!(serde_json::to_string(&empty).unwrap(), "[]");

    let nil = ContentOrParts::default();
    assert_eq!(serde_json::to_string(&nil).unwrap(), "null");
  }

  #[test]
  fn concurrent_marshal() {
    let val = Arc::new(ContentOrParts {
      parts: Some(vec![
        ContentPart::Text(text("hello")),
        ContentPart::Image(image("http://example.com")),
      ]),
      content: None,
    });
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
