use super::*;

impl Backend {
    pub(super) async fn handle_formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let key = params.text_document.uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        Ok(crate::format::formatting_edits(&text))
    }

    pub(super) async fn handle_range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let key = params.text_document.uri.to_string();
        let Some(text) = self.document_text(&key) else {
            return Ok(None);
        };
        Ok(crate::format::formatting_edits(&text))
    }
}
