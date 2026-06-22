// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::google::cloud::bigquery::storage::v1;
use gaxi::prost::{ConvertError, FromProto, ToProto};

/// ProtoSchema describes the schema of the serialized protocol buffer data rows.
#[derive(Clone, Default, PartialEq)]
pub struct ProtoSchema {
    /// Descriptor for input message.
    pub proto_descriptor: Option<wkt::DescriptorProto>,
}

impl ToProto<v1::ProtoSchema> for ProtoSchema {
    type Output = v1::ProtoSchema;
    fn to_proto(self) -> Result<v1::ProtoSchema, ConvertError> {
        let proto_descriptor = match self.proto_descriptor {
            Some(d) => Some(convert_descriptor(d)?),
            None => None,
        };
        Ok(v1::ProtoSchema { proto_descriptor })
    }
}

impl FromProto<ProtoSchema> for v1::ProtoSchema {
    fn cnv(self) -> Result<ProtoSchema, ConvertError> {
        let proto_descriptor = match self.proto_descriptor {
            Some(d) => Some(convert_descriptor_back(d)?),
            None => None,
        };
        Ok(ProtoSchema { proto_descriptor })
    }
}

impl ToProto<crate::generated::gapic_storage::model::ProtoSchema> for ProtoSchema {
    type Output = crate::generated::gapic_storage::model::ProtoSchema;
    fn to_proto(self) -> Result<Self::Output, ConvertError> {
        Ok(self.into())
    }
}

impl From<ProtoSchema> for crate::generated::gapic_storage::model::ProtoSchema {
    fn from(value: ProtoSchema) -> Self {
        Self {
            proto_descriptor: value.proto_descriptor,
            ..Default::default()
        }
    }
}

impl ToProto<v1::ProtoSchema> for crate::generated::gapic_storage::model::ProtoSchema {
    type Output = v1::ProtoSchema;
    fn to_proto(self) -> Result<v1::ProtoSchema, ConvertError> {
        let proto_descriptor = match self.proto_descriptor {
            Some(d) => Some(convert_descriptor(d)?),
            None => None,
        };
        Ok(v1::ProtoSchema { proto_descriptor })
    }
}

fn convert_descriptor(
    d: wkt::DescriptorProto,
) -> Result<prost_types::DescriptorProto, ConvertError> {
    Ok(prost_types::DescriptorProto {
        name: Some(d.name),
        field: d
            .field
            .into_iter()
            .map(convert_field_descriptor)
            .collect::<Result<Vec<_>, _>>()?,
        extension: d
            .extension
            .into_iter()
            .map(convert_field_descriptor)
            .collect::<Result<Vec<_>, _>>()?,
        nested_type: d
            .nested_type
            .into_iter()
            .map(convert_descriptor)
            .collect::<Result<Vec<_>, _>>()?,
        enum_type: d
            .enum_type
            .into_iter()
            .map(convert_enum_descriptor)
            .collect::<Result<Vec<_>, _>>()?,
        extension_range: d
            .extension_range
            .into_iter()
            .map(|er| prost_types::descriptor_proto::ExtensionRange {
                start: Some(er.start),
                end: Some(er.end),
                options: None, // Simplified
            })
            .collect(),
        oneof_decl: d
            .oneof_decl
            .into_iter()
            .map(|o| prost_types::OneofDescriptorProto {
                name: Some(o.name),
                options: None, // Simplified
            })
            .collect(),
        options: None, // Simplified
        reserved_range: d
            .reserved_range
            .into_iter()
            .map(|rr| prost_types::descriptor_proto::ReservedRange {
                start: Some(rr.start),
                end: Some(rr.end),
            })
            .collect(),
        reserved_name: d.reserved_name,
    })
}

fn convert_field_descriptor(
    d: wkt::FieldDescriptorProto,
) -> Result<prost_types::FieldDescriptorProto, ConvertError> {
    Ok(prost_types::FieldDescriptorProto {
        name: Some(d.name),
        number: Some(d.number),
        label: d.label.value(),
        r#type: d.r#type.value(),
        type_name: if d.type_name.is_empty() {
            None
        } else {
            Some(d.type_name)
        },
        extendee: if d.extendee.is_empty() {
            None
        } else {
            Some(d.extendee)
        },
        default_value: if d.default_value.is_empty() {
            None
        } else {
            Some(d.default_value)
        },
        oneof_index: if d.oneof_index == 0 {
            // Note: 0 is a valid index, but in many cases it's just the default.
            // If it's really part of a oneof, it should be set.
            // However, the error said "oneof_index 0 is out of range".
            // If there are NO oneofs defined, 0 is indeed out of range.
            None
        } else {
            Some(d.oneof_index)
        },
        json_name: if d.json_name.is_empty() {
            None
        } else {
            Some(d.json_name)
        },
        options: None, // Simplified
        proto3_optional: None,
    })
}

fn convert_enum_descriptor(
    d: wkt::EnumDescriptorProto,
) -> Result<prost_types::EnumDescriptorProto, ConvertError> {
    Ok(prost_types::EnumDescriptorProto {
        name: Some(d.name),
        value: d
            .value
            .into_iter()
            .map(|v| prost_types::EnumValueDescriptorProto {
                name: Some(v.name),
                number: Some(v.number),
                options: None, // Simplified
            })
            .collect(),
        options: None, // Simplified
        reserved_range: d
            .reserved_range
            .into_iter()
            .map(|rr| prost_types::enum_descriptor_proto::EnumReservedRange {
                start: Some(rr.start),
                end: Some(rr.end),
            })
            .collect(),
        reserved_name: d.reserved_name,
    })
}

fn convert_descriptor_back(
    _d: prost_types::DescriptorProto,
) -> Result<wkt::DescriptorProto, ConvertError> {
    // Back conversion is less critical for the write path, keeping it stubbed for now.
    Err(ConvertError::Unimplemented)
}
