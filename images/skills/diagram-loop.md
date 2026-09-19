---
name: claude-here-diagram-loop
description: Render a diagram and look at the result before handing it over. Use when writing or changing PlantUML, D2, Graphviz or Mermaid diagrams in a claude_here sandbox with the docs toolchain enabled.
---

# Render, look, fix

Diagram source that has never been rendered is a guess. When the `docs`
toolchain is enabled the sandbox can render, and the Read tool displays PNGs, so
close the loop instead of shipping unseen output.

* PlantUML: `plantuml -tpng diagram.puml` (Graphviz is in the base image, so
  class and component layouts work).
* D2: `d2 diagram.d2 diagram.png`.
* Graphviz: `dot -Tpng graph.dot -o graph.png`.
* Typst for a full document: `typst compile doc.typ`.
* Pandoc for format conversion: `pandoc README.md -o readme.pdf --pdf-engine=typst`.

Then **read the PNG** and judge it as a reader would:

1. Does any label overlap, clip, or leave the canvas?
2. Do the arrows say what the text claims — direction, source, target?
3. Is the depth right? A diagram that restates the file tree earns nothing; one
   that shows the mechanism does.
4. Is it legible at the size it will be viewed, and does it survive being read
   without colour?

Fix and render again. Hand over the source *and* the rendered file, and say
which renderer produced it.

Mermaid has no renderer here on purpose: GitHub renders ` ```mermaid ` blocks
from source, so emit the source and let the platform draw it.
