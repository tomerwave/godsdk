use godsdk_core::{ApiSpec, Schema};

#[test]
fn selects_response_media_types_in_canonical_order() {
    let spec = ApiSpec::parse(
        r##"
openapi: 3.1.1
info: {title: Media Types, version: 1.0.0}
paths:
  /pets:
    get:
      operationId: listPets
      responses:
        "200":
          content:
            text/plain: {schema: {type: string}}
            application/json: {schema: {$ref: "#/components/schemas/Pet"}}
components:
  schemas:
    Pet: {type: object, properties: {name: {type: string}}}
"##,
    )
    .unwrap_or_else(|error| panic!("document parses: {error}"));

    assert!(matches!(
        spec.operations[0].responses[0].schema,
        Some(Schema::Reference(ref name)) if name == "Pet"
    ));
}
