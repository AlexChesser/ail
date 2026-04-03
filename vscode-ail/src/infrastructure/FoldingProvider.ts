/**
 * FoldingProvider — implements vscode.FoldingRangeProvider for ail-log documents.
 *
 * Detects all `:::` directive blocks and returns folding ranges.
 * Respects the `ail.autoFoldThinking` configuration to auto-collapse
 * thinking blocks when a document is opened.
 */

import * as vscode from 'vscode';

export class AilLogFoldingProvider implements vscode.FoldingRangeProvider {
  constructor(private context: vscode.ExtensionContext) {}

  /**
   * Provide folding ranges for an ail-log document.
   * Detects all `:::` directive blocks and returns FoldingRange objects.
   */
  provideFoldingRanges(
    document: vscode.TextDocument,
    _context: vscode.FoldingContext,
    _token: vscode.CancellationToken
  ): vscode.FoldingRange[] {
    const ranges: vscode.FoldingRange[] = [];
    const text = document.getText();
    const lines = text.split('\n');

    let i = 0;
    while (i < lines.length) {
      const line = lines[i];

      // Match opening directive: :::directive-name ...
      const directiveMatch = line.match(/^:::[a-z-]+/);
      if (directiveMatch) {
        const startLine = i;
        // Find the closing :::
        let endLine = startLine + 1;
        while (endLine < lines.length) {
          if (lines[endLine].match(/^:::\s*$/)) {
            // Found closing marker
            ranges.push(new vscode.FoldingRange(startLine, endLine));
            i = endLine;
            break;
          }
          endLine++;
        }
        if (endLine >= lines.length) {
          // No closing marker found, skip this unclosed directive
          i++;
        }
      } else {
        i++;
      }
    }

    return ranges;
  }
}

/**
 * Auto-fold thinking blocks on document open if ail.autoFoldThinking is enabled.
 * Called when a virtual ail-log document is opened.
 *
 * @param editor The text editor containing the document
 */
export async function autoFoldThinkingBlocks(editor: vscode.TextEditor): Promise<void> {
  const config = vscode.workspace.getConfiguration('ail');
  const autoFold = config.get<boolean>('autoFoldThinking', true);

  if (!autoFold) {
    return;
  }

  const document = editor.document;
  const text = document.getText();
  const lines = text.split('\n');

  // Collect all thinking block ranges
  const thinkingRanges: vscode.Range[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    // Match opening thinking directive
    if (line.match(/^:::thinking\s*$/)) {
      const startLine = i;
      // Find the closing :::
      let endLine = startLine + 1;
      while (endLine < lines.length) {
        if (lines[endLine].match(/^:::\s*$/)) {
          // Found closing marker
          thinkingRanges.push(
            new vscode.Range(
              new vscode.Position(startLine, 0),
              new vscode.Position(endLine, 0)
            )
          );
          i = endLine;
          break;
        }
        endLine++;
      }
      if (endLine >= lines.length) {
        i++;
      }
    } else {
      i++;
    }
  }

  // Fold each thinking block by toggling the fold at its range
  // Use a small delay to allow the folding provider to initialize
  await new Promise(resolve => setTimeout(resolve, 100));

  for (const range of thinkingRanges) {
    try {
      await vscode.commands.executeCommand(
        'editor.toggleFold',
        { lineNumber: range.start.line }
      );
    } catch (err) {
      // Silently ignore fold errors; the document is still readable
      console.debug(`[ail] Failed to auto-fold thinking block at line ${range.start.line}`);
    }
  }
}
