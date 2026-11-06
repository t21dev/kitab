# Math

Inline math sits in the text line: the area of a circle is $A = \pi r^2$, and Euler's identity is $e^{i\pi} + 1 = 0$.

Display math is centred on its own line:

$$
\int_0^1 x^2 \, dx = \frac{1}{3}
$$

$$
\mathbf{V} = \begin{pmatrix} a & b \\ c & d \end{pmatrix}, \qquad \det \mathbf{V} = ad - bc
$$

A fenced `math` block renders the same way:

```math
\sum_{k=1}^{n} k = \frac{n(n+1)}{2}
```

Dollar signs that are not math stay as text: the book costs $12 and the other costs $15.

Invalid TeX must show the source with an error mark, not break the page: $\frac{1}{$.
