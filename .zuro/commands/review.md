---
description: Review code for quality, correctness and style
inputs:
  - name: focus
    prompt: "Focus area (optional): "
---
{% if stdin %}
{{ stdin }}

{% endif %}
Review this code. Comment on correctness, style, potential bugs, and improvements. Be specific.{% if inputs.focus %} Pay particular attention to: {{ inputs.focus }}.{% endif %}
