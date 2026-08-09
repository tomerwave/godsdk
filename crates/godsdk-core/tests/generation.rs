use godsdk_core::{GenerationError, GenerationRequest, generate};

#[test]
fn generation_reports_the_scaffold_boundary() {
    let request = GenerationRequest::new("spec.yaml", "out");

    let error = match generate(&request) {
        Ok(_) => panic!("the scaffold must not generate an SDK"),
        Err(error) => error,
    };

    assert_eq!(error, GenerationError::NotImplemented);
    assert_eq!(error.to_string(), "SDK generation is not implemented yet");
}
