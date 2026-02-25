---
description: Find and fix bugs in the provided code
inputs:
  - name: focus
    prompt: "Focus on (optional): "
---
{% if stdin %}
{{ stdin }}

{% endif %}
Find and fix all bugs in the above code.{% if inputs.focus %} Focus on: {{ inputs.focus }}.{% endif %} Show only changed sections with brief explanations.
