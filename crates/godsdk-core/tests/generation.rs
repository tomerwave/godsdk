use godsdk_core::{GenerationError, GenerationRequest, generate};

#[test]
fn generation_reports_the_scaffold_boundary() {
    let request = GenerationRequest::new("spec.yaml", "out");

    let error = generate(&request).expect_err("the scaffold must not generate an SDK");

    assert_eq!(error, GenerationError::NotImplemented);
    assert_eq!(error.to_string(), "SDK generation is not implemented yet");
}
