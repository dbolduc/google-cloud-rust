// Copyright 2025 Google LLC
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
//
// Code generated by sidekick. DO NOT EDIT.

pub mod access_approval {
    use crate::Result;
    use std::sync::Arc;

    /// Common implementation for [super::super::client::AccessApproval] request builders.
    #[derive(Clone, Debug)]
    pub(crate) struct RequestBuilder<R: std::default::Default> {
        stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>,
        request: R,
        options: gax::options::RequestOptions,
    }

    impl<R> RequestBuilder<R>
    where
        R: std::default::Default,
    {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self {
                stub,
                request: R::default(),
                options: gax::options::RequestOptions::default(),
            }
        }
    }

    /// The request builder for a AccessApproval::list_approval_requests call.
    #[derive(Clone, Debug)]
    pub struct ListApprovalRequests(RequestBuilder<crate::model::ListApprovalRequestsMessage>);

    impl ListApprovalRequests {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::ListApprovalRequestsMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<crate::model::ListApprovalRequestsResponse> {
            (*self.0.stub)
                .list_approval_requests(self.0.request, self.0.options)
                .await
        }

        /// Streams the responses back.
        pub async fn paginator(
            self,
        ) -> gax::paginator::Paginator<crate::model::ListApprovalRequestsResponse, gax::error::Error>
        {
            use std::clone::Clone;
            let token = self.0.request.page_token.clone();
            let execute = move |token: String| {
                let mut builder = self.clone();
                builder.0.request = builder.0.request.set_page_token(token);
                builder.send()
            };
            gax::paginator::Paginator::new(token, execute)
        }

        /// Sets the value of [parent][crate::model::ListApprovalRequestsMessage::parent].
        pub fn set_parent<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.parent = v.into();
            self
        }

        /// Sets the value of [filter][crate::model::ListApprovalRequestsMessage::filter].
        pub fn set_filter<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.filter = v.into();
            self
        }

        /// Sets the value of [page_size][crate::model::ListApprovalRequestsMessage::page_size].
        pub fn set_page_size<T: Into<i32>>(mut self, v: T) -> Self {
            self.0.request.page_size = v.into();
            self
        }

        /// Sets the value of [page_token][crate::model::ListApprovalRequestsMessage::page_token].
        pub fn set_page_token<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.page_token = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for ListApprovalRequests {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }

    /// The request builder for a AccessApproval::get_approval_request call.
    #[derive(Clone, Debug)]
    pub struct GetApprovalRequest(RequestBuilder<crate::model::GetApprovalRequestMessage>);

    impl GetApprovalRequest {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::GetApprovalRequestMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<crate::model::ApprovalRequest> {
            (*self.0.stub)
                .get_approval_request(self.0.request, self.0.options)
                .await
        }

        /// Sets the value of [name][crate::model::GetApprovalRequestMessage::name].
        pub fn set_name<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.name = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for GetApprovalRequest {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }

    /// The request builder for a AccessApproval::approve_approval_request call.
    #[derive(Clone, Debug)]
    pub struct ApproveApprovalRequest(RequestBuilder<crate::model::ApproveApprovalRequestMessage>);

    impl ApproveApprovalRequest {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::ApproveApprovalRequestMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<crate::model::ApprovalRequest> {
            (*self.0.stub)
                .approve_approval_request(self.0.request, self.0.options)
                .await
        }

        /// Sets the value of [name][crate::model::ApproveApprovalRequestMessage::name].
        pub fn set_name<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.name = v.into();
            self
        }

        /// Sets the value of [expire_time][crate::model::ApproveApprovalRequestMessage::expire_time].
        pub fn set_expire_time<T: Into<std::option::Option<wkt::Timestamp>>>(
            mut self,
            v: T,
        ) -> Self {
            self.0.request.expire_time = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for ApproveApprovalRequest {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }

    /// The request builder for a AccessApproval::dismiss_approval_request call.
    #[derive(Clone, Debug)]
    pub struct DismissApprovalRequest(RequestBuilder<crate::model::DismissApprovalRequestMessage>);

    impl DismissApprovalRequest {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::DismissApprovalRequestMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<crate::model::ApprovalRequest> {
            (*self.0.stub)
                .dismiss_approval_request(self.0.request, self.0.options)
                .await
        }

        /// Sets the value of [name][crate::model::DismissApprovalRequestMessage::name].
        pub fn set_name<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.name = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for DismissApprovalRequest {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }

    /// The request builder for a AccessApproval::invalidate_approval_request call.
    #[derive(Clone, Debug)]
    pub struct InvalidateApprovalRequest(
        RequestBuilder<crate::model::InvalidateApprovalRequestMessage>,
    );

    impl InvalidateApprovalRequest {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::InvalidateApprovalRequestMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<crate::model::ApprovalRequest> {
            (*self.0.stub)
                .invalidate_approval_request(self.0.request, self.0.options)
                .await
        }

        /// Sets the value of [name][crate::model::InvalidateApprovalRequestMessage::name].
        pub fn set_name<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.name = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for InvalidateApprovalRequest {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }

    /// The request builder for a AccessApproval::get_access_approval_settings call.
    #[derive(Clone, Debug)]
    pub struct GetAccessApprovalSettings(
        RequestBuilder<crate::model::GetAccessApprovalSettingsMessage>,
    );

    impl GetAccessApprovalSettings {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::GetAccessApprovalSettingsMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<crate::model::AccessApprovalSettings> {
            (*self.0.stub)
                .get_access_approval_settings(self.0.request, self.0.options)
                .await
        }

        /// Sets the value of [name][crate::model::GetAccessApprovalSettingsMessage::name].
        pub fn set_name<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.name = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for GetAccessApprovalSettings {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }

    /// The request builder for a AccessApproval::update_access_approval_settings call.
    #[derive(Clone, Debug)]
    pub struct UpdateAccessApprovalSettings(
        RequestBuilder<crate::model::UpdateAccessApprovalSettingsMessage>,
    );

    impl UpdateAccessApprovalSettings {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::UpdateAccessApprovalSettingsMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<crate::model::AccessApprovalSettings> {
            (*self.0.stub)
                .update_access_approval_settings(self.0.request, self.0.options)
                .await
        }

        /// Sets the value of [settings][crate::model::UpdateAccessApprovalSettingsMessage::settings].
        pub fn set_settings<T: Into<std::option::Option<crate::model::AccessApprovalSettings>>>(
            mut self,
            v: T,
        ) -> Self {
            self.0.request.settings = v.into();
            self
        }

        /// Sets the value of [update_mask][crate::model::UpdateAccessApprovalSettingsMessage::update_mask].
        pub fn set_update_mask<T: Into<std::option::Option<wkt::FieldMask>>>(
            mut self,
            v: T,
        ) -> Self {
            self.0.request.update_mask = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for UpdateAccessApprovalSettings {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }

    /// The request builder for a AccessApproval::delete_access_approval_settings call.
    #[derive(Clone, Debug)]
    pub struct DeleteAccessApprovalSettings(
        RequestBuilder<crate::model::DeleteAccessApprovalSettingsMessage>,
    );

    impl DeleteAccessApprovalSettings {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::DeleteAccessApprovalSettingsMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<wkt::Empty> {
            (*self.0.stub)
                .delete_access_approval_settings(self.0.request, self.0.options)
                .await
        }

        /// Sets the value of [name][crate::model::DeleteAccessApprovalSettingsMessage::name].
        pub fn set_name<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.name = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for DeleteAccessApprovalSettings {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }

    /// The request builder for a AccessApproval::get_access_approval_service_account call.
    #[derive(Clone, Debug)]
    pub struct GetAccessApprovalServiceAccount(
        RequestBuilder<crate::model::GetAccessApprovalServiceAccountMessage>,
    );

    impl GetAccessApprovalServiceAccount {
        pub(crate) fn new(stub: Arc<dyn super::super::stubs::dynamic::AccessApproval>) -> Self {
            Self(RequestBuilder::new(stub))
        }

        /// Sets the full request, replacing any prior values.
        pub fn with_request<V: Into<crate::model::GetAccessApprovalServiceAccountMessage>>(
            mut self,
            v: V,
        ) -> Self {
            self.0.request = v.into();
            self
        }

        /// Sets all the options, replacing any prior values.
        pub fn with_options<V: Into<gax::options::RequestOptions>>(mut self, v: V) -> Self {
            self.0.options = v.into();
            self
        }

        /// Sends the request.
        pub async fn send(self) -> Result<crate::model::AccessApprovalServiceAccount> {
            (*self.0.stub)
                .get_access_approval_service_account(self.0.request, self.0.options)
                .await
        }

        /// Sets the value of [name][crate::model::GetAccessApprovalServiceAccountMessage::name].
        pub fn set_name<T: Into<std::string::String>>(mut self, v: T) -> Self {
            self.0.request.name = v.into();
            self
        }
    }

    impl gax::options::RequestBuilder for GetAccessApprovalServiceAccount {
        fn request_options(&mut self) -> &mut gax::options::RequestOptions {
            &mut self.0.options
        }
    }
}
