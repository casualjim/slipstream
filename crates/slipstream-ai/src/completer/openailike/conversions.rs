use async_openai::types::{
  ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
  ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestAssistantMessageContentPart,
  ChatCompletionRequestDeveloperMessage, ChatCompletionRequestDeveloperMessageContent,
  ChatCompletionRequestMessage, ChatCompletionRequestMessageContentPartAudio,
  ChatCompletionRequestMessageContentPartImage, ChatCompletionRequestMessageContentPartRefusal,
  ChatCompletionRequestMessageContentPartText, ChatCompletionRequestSystemMessage,
  ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
  ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessage,
  ChatCompletionRequestUserMessageContent, ChatCompletionRequestUserMessageContentPart,
  ChatCompletionToolType, FunctionCall, ImageUrl, InputAudio, InputAudioFormat,
};
use base64::{Engine as _, engine::general_purpose};

use crate::messages::{
  AssistantContentOrParts, AssistantContentPart, AssistantMessage, ContentOrParts, ContentPart,
  InstructionsMessage, InstructionsType, Message, ModelMessage, Request, Response, Retry,
  ToolCallData, ToolCallMessage, ToolResponse, UserMessage,
};

pub fn messages_to_openai(
  messages: &Vec<Message<ModelMessage>>,
) -> Vec<ChatCompletionRequestMessage> {
  messages.iter().map(message_to_openai).collect()
}

pub fn message_to_openai(msg: &Message<ModelMessage>) -> ChatCompletionRequestMessage {
  match &msg.payload {
    ModelMessage::Request(Request::User(UserMessage { content })) => {
      ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
        content: content_or_parts_to_openai(content.clone()),
        name: msg.sender.clone(),
      })
    }
    ModelMessage::Request(Request::ToolResponse(ToolResponse {
      tool_call_id,
      content,
      ..
    })) => ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
      content: content.clone().into(),
      tool_call_id: tool_call_id.clone(),
    }),
    ModelMessage::Request(Request::Retry(Retry {
      error,
      tool_call_id: None,
      ..
    })) => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
      content: ChatCompletionRequestUserMessageContent::Text(error.clone()),
      name: msg.sender.clone(),
    }),
    ModelMessage::Request(Request::Retry(Retry {
      error,
      tool_call_id: Some(tool_call_id),
      ..
    })) => ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
      content: ChatCompletionRequestToolMessageContent::Text(error.clone()),
      tool_call_id: tool_call_id.clone(),
    }),
    ModelMessage::Instructions(InstructionsMessage {
      content,
      type_field,
    }) => match type_field {
      InstructionsType::Developer => {
        ChatCompletionRequestMessage::Developer(ChatCompletionRequestDeveloperMessage {
          content: ChatCompletionRequestDeveloperMessageContent::Text(content.clone()),
          name: msg.sender.clone(),
        })
      }
      _ => ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
        content: ChatCompletionRequestSystemMessageContent::Text(content.clone()),
        name: msg.sender.clone(),
      }),
    },
    ModelMessage::Response(Response::Assistant(AssistantMessage { content, refusal })) => {
      ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
        content: content
          .as_ref()
          .map(|v| assistant_content_or_parts_to_openai(v.clone())),
        refusal: refusal.as_ref().map(|r| r.clone().into()),
        name: msg.sender.clone(),
        ..Default::default()
      })
    }
    ModelMessage::Response(Response::ToolCall(ToolCallMessage { tool_calls })) => {
      ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
        tool_calls: Some(
          tool_calls
            .into_iter()
            .map(|v| tool_call_to_openai(v.clone()))
            .collect::<Vec<_>>(),
        ),
        name: msg.sender.clone(),
        ..Default::default()
      })
    }
  }
}

fn tool_call_to_openai(tool_call: ToolCallData) -> ChatCompletionMessageToolCall {
  ChatCompletionMessageToolCall {
    id: tool_call.id,
    r#type: ChatCompletionToolType::Function,
    function: FunctionCall {
      name: tool_call.name,
      arguments: tool_call.arguments,
    },
  }
}

fn content_part_to_openai(part: ContentPart) -> ChatCompletionRequestUserMessageContentPart {
  match part {
    ContentPart::Text(text) => ChatCompletionRequestUserMessageContentPart::Text(
      ChatCompletionRequestMessageContentPartText { text: text.text },
    ),
    ContentPart::Image(image) => ChatCompletionRequestUserMessageContentPart::ImageUrl(
      ChatCompletionRequestMessageContentPartImage {
        image_url: ImageUrl {
          url: image.url,
          detail: None,
        },
      },
    ),
    ContentPart::Audio(audio) => ChatCompletionRequestUserMessageContentPart::InputAudio(
      ChatCompletionRequestMessageContentPartAudio {
        input_audio: InputAudio {
          data: general_purpose::STANDARD.encode(&audio.input_audio.data),
          format: match audio.input_audio.format.as_str() {
            "mp3" => InputAudioFormat::Mp3,
            "wav" => InputAudioFormat::Wav,
            _ => panic!("Unsupported audio format: {}", audio.input_audio.format),
          },
        },
      },
    ),
    ContentPart::Video(video) => {
      unimplemented!("Video content not supported in OpenAI API: {:?}", video)
    }
  }
}

fn assistant_content_part_to_openai(
  part: AssistantContentPart,
) -> ChatCompletionRequestAssistantMessageContentPart {
  match part {
    AssistantContentPart::Text(text) => ChatCompletionRequestAssistantMessageContentPart::Text(
      ChatCompletionRequestMessageContentPartText { text: text.text },
    ),
    AssistantContentPart::Refusal(refusal) => {
      ChatCompletionRequestAssistantMessageContentPart::Refusal(
        ChatCompletionRequestMessageContentPartRefusal {
          refusal: refusal.refusal,
        },
      )
    }
  }
}

fn content_or_parts_to_openai(content: ContentOrParts) -> ChatCompletionRequestUserMessageContent {
  match content {
    ContentOrParts::Content(text) => ChatCompletionRequestUserMessageContent::Text(text),
    ContentOrParts::Parts(parts) => ChatCompletionRequestUserMessageContent::Array(
      parts.into_iter().map(content_part_to_openai).collect(),
    ),
  }
}

fn assistant_content_or_parts_to_openai(
  content: AssistantContentOrParts,
) -> ChatCompletionRequestAssistantMessageContent {
  match content {
    AssistantContentOrParts::Content(text) => {
      ChatCompletionRequestAssistantMessageContent::Text(text)
    }
    AssistantContentOrParts::Refusal(text) => {
      ChatCompletionRequestAssistantMessageContent::Array(vec![
        ChatCompletionRequestAssistantMessageContentPart::Refusal(
          ChatCompletionRequestMessageContentPartRefusal { refusal: text },
        ),
      ])
    }
    AssistantContentOrParts::Parts(parts) => ChatCompletionRequestAssistantMessageContent::Array(
      parts
        .into_iter()
        .map(assistant_content_part_to_openai)
        .collect(),
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::messages::{
    AudioContentPart, Erasable, ImageContentPart, MessageBuilder, RefusalContentPart,
    TextContentPart,
  };
  use serde_json::json;
  use slipstream_core::messages::{InputAudio, InputVideo, VideoContentPart};

  #[test]
  fn test_user_message_simple_text_conversion() {
    let msg = MessageBuilder::new()
      .with_sender("user123")
      .user_prompt("Hello, world!")
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::User(user_msg) => {
        assert_eq!(user_msg.name, Some("user123".to_string()));
        match user_msg.content {
          ChatCompletionRequestUserMessageContent::Text(text) => {
            assert_eq!(text, "Hello, world!");
          }
          _ => panic!("Expected Text content"),
        }
      }
      _ => panic!("Expected User message"),
    }
  }

  #[test]
  fn test_user_message_multipart_conversion() {
    let parts = vec![
      ContentPart::Text(TextContentPart {
        text: "Check out this image:".to_string(),
      }),
      ContentPart::Image(ImageContentPart {
        url: "https://example.com/image.jpg".to_string(),
        detail: Some("high".to_string()),
      }),
      ContentPart::Text(TextContentPart {
        text: "What do you see?".to_string(),
      }),
    ];

    let msg = MessageBuilder::new()
      .with_sender("user")
      .user_prompt_multipart(parts)
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::User(user_msg) => {
        match user_msg.content {
          ChatCompletionRequestUserMessageContent::Array(parts) => {
            assert_eq!(parts.len(), 3);

            // Check first text part
            match &parts[0] {
              ChatCompletionRequestUserMessageContentPart::Text(text) => {
                assert_eq!(text.text, "Check out this image:");
              }
              _ => panic!("Expected text part at index 0"),
            }

            // Check image part
            match &parts[1] {
              ChatCompletionRequestUserMessageContentPart::ImageUrl(image) => {
                assert_eq!(image.image_url.url, "https://example.com/image.jpg");
                assert_eq!(image.image_url.detail, None); // Note: detail is not preserved in conversion
              }
              _ => panic!("Expected image part at index 1"),
            }

            // Check second text part
            match &parts[2] {
              ChatCompletionRequestUserMessageContentPart::Text(text) => {
                assert_eq!(text.text, "What do you see?");
              }
              _ => panic!("Expected text part at index 2"),
            }
          }
          _ => panic!("Expected Array content"),
        }
      }
      _ => panic!("Expected User message"),
    }
  }

  #[test]
  fn test_user_message_with_audio_conversion() {
    let parts = vec![
      ContentPart::Text(TextContentPart {
        text: "Listen to this:".to_string(),
      }),
      ContentPart::Audio(AudioContentPart {
        input_audio: InputAudio {
          data: vec![1, 2, 3, 4, 5], // Mock audio data
          format: "mp3".to_string(),
        },
      }),
    ];

    let msg = MessageBuilder::new().user_prompt_multipart(parts).erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::User(user_msg) => {
        match user_msg.content {
          ChatCompletionRequestUserMessageContent::Array(parts) => {
            assert_eq!(parts.len(), 2);

            match &parts[1] {
              ChatCompletionRequestUserMessageContentPart::InputAudio(audio) => {
                // Verify base64 encoding
                let expected_b64 = general_purpose::STANDARD.encode(&[1, 2, 3, 4, 5]);
                assert_eq!(audio.input_audio.data, expected_b64);
                assert_eq!(audio.input_audio.format, InputAudioFormat::Mp3);
              }
              _ => panic!("Expected audio part at index 1"),
            }
          }
          _ => panic!("Expected Array content"),
        }
      }
      _ => panic!("Expected User message"),
    }
  }

  #[test]
  fn test_assistant_message_with_content() {
    let msg = MessageBuilder::new()
      .with_sender("gpt-4")
      .assistant_message("I can help with that!")
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::Assistant(assistant_msg) => {
        assert_eq!(assistant_msg.name, Some("gpt-4".to_string()));
        assert!(assistant_msg.refusal.is_none());
        assert!(assistant_msg.tool_calls.is_none());

        match assistant_msg.content {
          Some(ChatCompletionRequestAssistantMessageContent::Text(text)) => {
            assert_eq!(text, "I can help with that!");
          }
          _ => panic!("Expected Text content"),
        }
      }
      _ => panic!("Expected Assistant message"),
    }
  }

  #[test]
  fn test_assistant_message_with_refusal() {
    let msg = MessageBuilder::new()
      .with_sender("assistant")
      .assistant_refusal("I cannot do that")
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::Assistant(assistant_msg) => {
        assert_eq!(assistant_msg.refusal, Some("I cannot do that".to_string()));
        assert!(assistant_msg.content.is_none());
      }
      _ => panic!("Expected Assistant message"),
    }
  }

  #[test]
  fn test_assistant_message_multipart() {
    let parts = vec![
      AssistantContentPart::Text(TextContentPart {
        text: "Here's my response:".to_string(),
      }),
      AssistantContentPart::Refusal(RefusalContentPart {
        refusal: "But I can't do the illegal part".to_string(),
      }),
    ];

    let msg = MessageBuilder::new()
      .with_sender("assistant")
      .assistant_message_multipart(parts)
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::Assistant(assistant_msg) => match assistant_msg.content {
        Some(ChatCompletionRequestAssistantMessageContent::Array(parts)) => {
          assert_eq!(parts.len(), 2);

          match &parts[0] {
            ChatCompletionRequestAssistantMessageContentPart::Text(text) => {
              assert_eq!(text.text, "Here's my response:");
            }
            _ => panic!("Expected text part at index 0"),
          }

          match &parts[1] {
            ChatCompletionRequestAssistantMessageContentPart::Refusal(refusal) => {
              assert_eq!(refusal.refusal, "But I can't do the illegal part");
            }
            _ => panic!("Expected refusal part at index 1"),
          }
        }
        _ => panic!("Expected Array content"),
      },
      _ => panic!("Expected Assistant message"),
    }
  }

  #[test]
  fn test_tool_call_conversion() {
    let tool_data = ToolCallData {
      id: "call-123".to_string(),
      name: "get_weather".to_string(),
      arguments: r#"{"location": "San Francisco"}"#.to_string(),
    };

    let msg = MessageBuilder::new()
      .with_sender("assistant")
      .tool_call(vec![tool_data.clone()])
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::Assistant(assistant_msg) => {
        assert_eq!(assistant_msg.name, Some("assistant".to_string()));
        assert!(assistant_msg.content.is_none());
        assert!(assistant_msg.refusal.is_none());

        let tool_calls = assistant_msg.tool_calls.expect("Expected tool_calls");
        assert_eq!(tool_calls.len(), 1);

        let tool_call = &tool_calls[0];
        assert_eq!(tool_call.id, "call-123");
        assert_eq!(tool_call.r#type, ChatCompletionToolType::Function);
        assert_eq!(tool_call.function.name, "get_weather");
        assert_eq!(
          tool_call.function.arguments,
          r#"{"location": "San Francisco"}"#
        );
      }
      _ => panic!("Expected Assistant message with tool calls"),
    }
  }

  #[test]
  fn test_multiple_tool_calls_conversion() {
    let tool_calls = vec![
      ToolCallData {
        id: "call-123".to_string(),
        name: "get_weather".to_string(),
        arguments: r#"{"location": "San Francisco"}"#.to_string(),
      },
      ToolCallData {
        id: "call-456".to_string(),
        name: "get_time".to_string(),
        arguments: r#"{"timezone": "PST"}"#.to_string(),
      },
    ];

    let msg = MessageBuilder::new()
      .with_sender("assistant")
      .tool_call(tool_calls)
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::Assistant(assistant_msg) => {
        let tool_calls = assistant_msg.tool_calls.expect("Expected tool_calls");
        assert_eq!(tool_calls.len(), 2);

        assert_eq!(tool_calls[0].id, "call-123");
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(
          tool_calls[0].function.arguments,
          r#"{"location": "San Francisco"}"#
        );

        assert_eq!(tool_calls[1].id, "call-456");
        assert_eq!(tool_calls[1].function.name, "get_time");
        assert_eq!(tool_calls[1].function.arguments, r#"{"timezone": "PST"}"#);
      }
      _ => panic!("Expected Assistant message with tool calls"),
    }
  }

  #[test]
  fn test_tool_response_conversion() {
    let msg = MessageBuilder::new()
      .with_sender("tool")
      .tool_response("call-123", "get_weather", "Sunny, 72°F")
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::Tool(tool_msg) => {
        match tool_msg.content {
          ChatCompletionRequestToolMessageContent::Text(text) => {
            assert_eq!(text, "Sunny, 72°F");
          }
          _ => panic!("Expected Text content"),
        }
        assert_eq!(tool_msg.tool_call_id, "call-123");
      }
      _ => panic!("Expected Tool message"),
    }
  }

  #[test]
  fn test_instructions_message_conversion() {
    let msg = MessageBuilder::new()
      .with_sender("system")
      .instructions("You are a helpful assistant.");

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::System(system_msg) => {
        assert_eq!(system_msg.name, Some("system".to_string()));
        match system_msg.content {
          ChatCompletionRequestSystemMessageContent::Text(text) => {
            assert_eq!(text, "You are a helpful assistant.");
          }
          _ => panic!("Expected Text content"),
        }
      }
      _ => panic!("Expected System message"),
    }
  }

  #[test]
  fn test_retry_message_with_tool_id_conversion() {
    let msg = MessageBuilder::new()
      .with_sender("tool")
      .tool_error("call-123", "get_weather", "API rate limit exceeded")
      .erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::Tool(tool_msg) => {
        match tool_msg.content {
          ChatCompletionRequestToolMessageContent::Text(text) => {
            assert_eq!(text, "API rate limit exceeded");
          }
          _ => panic!("Expected Text content"),
        }
        assert_eq!(tool_msg.tool_call_id, "call-123");
      }
      _ => panic!("Expected Tool message for retry with tool_id"),
    }
  }

  #[test]
  fn test_tool_call_to_openai_conversion() {
    let tool_call = ToolCallData {
      id: "call-789".to_string(),
      name: "search_web".to_string(),
      arguments: r#"{"query": "rust programming"}"#.to_string(),
    };

    let openai_tool_call = tool_call_to_openai(tool_call);

    assert_eq!(openai_tool_call.id, "call-789");
    assert_eq!(openai_tool_call.r#type, ChatCompletionToolType::Function);
    assert_eq!(openai_tool_call.function.name, "search_web");
    assert_eq!(
      openai_tool_call.function.arguments,
      r#"{"query": "rust programming"}"#
    );
  }

  #[test]
  fn test_messages_to_openai_batch_conversion() {
    let messages = vec![
      MessageBuilder::new()
        .with_sender("system")
        .instructions("You are helpful"),
      MessageBuilder::new()
        .with_sender("user")
        .user_prompt("Hello")
        .erase(),
      MessageBuilder::new()
        .with_sender("assistant")
        .assistant_message("Hi there!")
        .erase(),
      MessageBuilder::new()
        .with_sender("user")
        .user_prompt("Tell me about Rust")
        .erase(),
    ];

    let openai_messages = messages_to_openai(&messages);

    assert_eq!(openai_messages.len(), 4);

    // Verify each message type
    match &openai_messages[0] {
      ChatCompletionRequestMessage::System(_) => {}
      _ => panic!("Expected System message at index 0"),
    }

    match &openai_messages[1] {
      ChatCompletionRequestMessage::User(_) => {}
      _ => panic!("Expected User message at index 1"),
    }

    match &openai_messages[2] {
      ChatCompletionRequestMessage::Assistant(_) => {}
      _ => panic!("Expected Assistant message at index 2"),
    }

    match &openai_messages[3] {
      ChatCompletionRequestMessage::User(_) => {}
      _ => panic!("Expected User message at index 3"),
    }
  }

  #[test]
  fn test_assistant_content_or_parts_variants() {
    // Test direct refusal variant
    let refusal_content = AssistantContentOrParts::Refusal("Cannot comply".to_string());
    let openai_content = assistant_content_or_parts_to_openai(refusal_content);

    match openai_content {
      ChatCompletionRequestAssistantMessageContent::Array(parts) => {
        assert_eq!(parts.len(), 1);
        match &parts[0] {
          ChatCompletionRequestAssistantMessageContentPart::Refusal(r) => {
            assert_eq!(r.refusal, "Cannot comply");
          }
          _ => panic!("Expected refusal part"),
        }
      }
      _ => panic!("Expected Array content for refusal"),
    }
  }

  #[test]
  fn test_edge_case_empty_parts() {
    let msg = MessageBuilder::new().user_prompt_multipart(vec![]).erase();

    let openai_msg = message_to_openai(&msg);

    match openai_msg {
      ChatCompletionRequestMessage::User(user_msg) => match user_msg.content {
        ChatCompletionRequestUserMessageContent::Array(parts) => {
          assert_eq!(parts.len(), 0);
        }
        _ => panic!("Expected Array content"),
      },
      _ => panic!("Expected User message"),
    }
  }

  #[test]
  fn test_message_metadata_preservation() {
    use jiff::Timestamp;
    use uuid::Uuid;

    let run_id = Uuid::now_v7();
    let turn_id = Uuid::now_v7();
    let timestamp = Timestamp::now();
    let meta = json!({"key": "value"});

    let msg = MessageBuilder::new()
      .with_run_id(run_id)
      .with_turn_id(turn_id)
      .with_sender("user")
      .with_timestamp(timestamp)
      .with_meta(meta.clone())
      .user_prompt("Test message");

    // Convert to OpenAI format - metadata is lost in conversion
    let openai_msg = message_to_openai(&msg.clone().erase());

    // Verify original message still has metadata
    assert_eq!(msg.run_id, Some(run_id));
    assert_eq!(msg.turn_id, Some(turn_id));
    assert_eq!(msg.sender, Some("user".to_string()));
    assert_eq!(msg.timestamp, Some(timestamp));
    assert_eq!(msg.meta, Some(meta));

    // Verify sender name is preserved in OpenAI format
    match openai_msg {
      ChatCompletionRequestMessage::User(user_msg) => {
        assert_eq!(user_msg.name, Some("user".to_string()));
      }
      _ => panic!("Expected User message"),
    }
  }

  #[test]
  #[should_panic(expected = "Unsupported audio format: ogg")]
  fn test_unsupported_audio_format() {
    let parts = vec![ContentPart::Audio(AudioContentPart {
      input_audio: InputAudio {
        data: vec![1, 2, 3],
        format: "ogg".to_string(), // Unsupported format
      },
    })];

    let msg = MessageBuilder::new().user_prompt_multipart(parts).erase();

    message_to_openai(&msg); // Should panic
  }

  #[test]
  #[should_panic(expected = "Video content not supported in OpenAI API")]
  fn test_video_content_not_supported() {
    let parts = vec![ContentPart::Video(VideoContentPart {
      input_video: InputVideo {
        data: vec![],
        format: "mp4".to_string(),
      },
    })];

    let msg = MessageBuilder::new().user_prompt_multipart(parts).erase();

    message_to_openai(&msg); // Should panic
  }
}
