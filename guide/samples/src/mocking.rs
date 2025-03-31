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

pub mod unary {
    // ANCHOR: all
    // ANCHOR: use
    use google_cloud_speech_v2 as speech;
    use google_cloud_gax as gax;
    // ANCHOR_END: use

    // ANCHOR: my_application_function
    // An example application function.
    //
    // It makes an RPC, setting some field. In this case, it is the `GetRecognizer`
    // RPC, setting the name field.
    //
    // It processes the response from the server. In this case, it extracts the
    // display name of the recognizer.
    async fn my_application_function(client: &speech::client::Speech) -> gax::Result<String> {
        client.get_recognizer("invalid-test-recognizer").send().await
            .map(|r| r.display_name)
    }
    // ANCHOR_END: my_application_function

    // ANCHOR: mockall_macro
    mockall::mock! {
        #[derive(Debug)]
        Speech {}
        impl speech::stub::Speech for Speech {
            async fn get_recognizer(&self, req: speech::model::GetRecognizerRequest, _options: gax::options::RequestOptions) -> gax::Result<speech::model::Recognizer>;
        }
    }
    // ANCHOR_END: mockall_macro

    pub async fn unary() -> crate::Result<()> {
        // Create a mock, and set expectations on it.
        // ANCHOR: mock_new
        let mut mock = MockSpeech::new();
        // ANCHOR_END: mock_new
        // ANCHOR: mock_expectation
        mock.expect_get_recognizer()
            .withf(move |r, _|
                // Optionally, verify fields in the request.
                r.name == "invalid-test-recognizer"
            )
            .return_once(move |_, _| {
                Ok(speech::model::Recognizer::new().set_display_name("test-display-name"))
            });
        // ANCHOR_END: mock_expectation

        // Create a client, implemented by the mock.
        // ANCHOR: client_from_mock
        let client = speech::client::Speech::from_stub(mock);
        // ANCHOR_END: client_from_mock
        
        // Call our function.
        // ANCHOR: call_fn
        let display_name = my_application_function(&client).await?;
        // ANCHOR_END: call_fn

        // Verify the final result of the RPC.
        // ANCHOR: validate
        assert_eq!(display_name, "test-display-name");
        // ANCHOR_END: validate

        Ok(())
    }
    // ANCHOR_END: all
}

pub mod lro {
    // ANCHOR: lro_use
    use google_cloud_longrunning as longrunning;
    use google_cloud_speech_v2 as speech;
    use google_cloud_gax as gax;
    use google_cloud_wkt as wkt;
    // ANCHOR_END: lro_use

    // ANCHOR: lro_mockall_macro
    mockall::mock! {
        #[derive(Debug)]
        Speech {}
        impl speech::stub::Speech for Speech {
            async fn get_recognizer(&self, req: speech::model::GetRecognizerRequest, _options: gax::options::RequestOptions) -> gax::Result<speech::model::Recognizer>;
            async fn batch_recognize(&self, req: speech::model::BatchRecognizeRequest, _options: gax::options::RequestOptions) -> gax::Result<longrunning::model::Operation>;
        }
    }
    // ANCHOR_END: lro_mockall_macro

    // ANCHOR: lro_finished_operation_helper
    fn make_finished_operation(
        response: &speech::model::BatchRecognizeResponse,
    ) -> crate::Result<longrunning::model::Operation> {
        let any = wkt::Any::try_from(response)?;
        let operation = longrunning::model::Operation::new()
            .set_done(true)
            .set_result(longrunning::model::operation::Result::Response(any.into()));
        Ok(operation)
    }
    // ANCHOR_END: lro_finished_operation_helper

    pub async fn lro() -> crate::Result<()> {
        use speech::Poller;

        let response = speech::model::BatchRecognizeResponse::new()
            .set_total_billed_duration(wkt::Duration::clamp(1, 0));
        let response_clone = response.clone();

        let mut mock = MockSpeech::new();
        mock.expect_batch_recognize()
            .withf(move |r, _|
                // Verify fields in the request.
                r.recognizer == "invalid-test-recognizer"
            )
            .return_once(move |_, _| {
                make_finished_operation(&response_clone).map_err(gax::error::Error::serde)
            });

        // Create a client, implemented by our mock.
        let client = speech::client::Speech::from_stub(mock);

        let actual = client
            .batch_recognize("invalid-test-recognizer")
            .poller()
            .until_done()
            .await?;

        // Verify the final result of the LRO.
        assert_eq!(response, actual);

        Ok(())
    }
}