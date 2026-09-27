"""Arithmetic Quantix does for the office on request, so no sum is worked out in a model's head: an expression with
named values, and earthwork volumes from a grid of levels."""

import ast
import operator
from decimal import Decimal, InvalidOperation

_BINARY = {
    ast.Add: operator.add,
    ast.Sub: operator.sub,
    ast.Mult: operator.mul,
    ast.Div: operator.truediv,
    ast.Pow: operator.pow,
}
_UNARY = {ast.UAdd: operator.pos, ast.USub: operator.neg}
_ALLOWED = "Use numbers, named values, + - * / ** and brackets, and min, max, abs or sqrt."


def evaluate(expression: str, values: dict[str, Decimal] | None = None) -> Decimal:
    """The value of an arithmetic expression such as "L * W * D", with L, W and D given in values."""
    if len(expression) > 500:
        raise ValueError("Break a long calculation into steps of up to 500 characters.")
    named = {name: Decimal(str(value)) for name, value in (values or {}).items()}
    try:
        tree = ast.parse(expression.replace("×", "*").replace("÷", "/"), mode="eval")
    except SyntaxError as error:
        raise ValueError(f"That isn't an expression Quantix can work out. {_ALLOWED}") from error

    def walk(node: ast.AST) -> Decimal:
        if isinstance(node, ast.Expression):
            return walk(node.body)
        if isinstance(node, ast.Constant) and type(node.value) in (int, float):
            return Decimal(str(node.value))
        if isinstance(node, ast.Name):
            if node.id not in named:
                raise ValueError(f"{node.id} has no value: give it in values.")
            return named[node.id]
        if isinstance(node, ast.BinOp) and type(node.op) in _BINARY:
            left, right = walk(node.left), walk(node.right)
            if isinstance(node.op, ast.Div) and right == 0:
                raise ValueError("That divides by zero.")
            if isinstance(node.op, ast.Pow) and (right != right.to_integral_value() or abs(right) > 10):
                raise ValueError("Powers are whole numbers up to 10; use sqrt for a square root.")
            return _BINARY[type(node.op)](left, right)
        if isinstance(node, ast.UnaryOp) and type(node.op) in _UNARY:
            return _UNARY[type(node.op)](walk(node.operand))
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and not node.keywords and node.args:
            args = [walk(a) for a in node.args]
            if node.func.id == "sqrt" and len(args) == 1 and args[0] >= 0:
                return args[0].sqrt()
            if node.func.id in ("min", "max"):
                return (min if node.func.id == "min" else max)(args)
            if node.func.id == "abs" and len(args) == 1:
                return abs(args[0])
        raise ValueError(_ALLOWED)

    try:
        return walk(tree)
    except (InvalidOperation, OverflowError) as error:
        raise ValueError("The numbers are out of range.") from error


def plain(value: Decimal) -> str:
    """A result to at most 4 decimal places, without trailing zeros."""
    return f"{value.quantize(Decimal('0.0001')):,}".rstrip("0").rstrip(".")


def grid_volumes(
    spacing: Decimal, levels: list[list[Decimal]], formation: list[list[Decimal]]
) -> tuple[Decimal, Decimal, int]:
    """Cut and fill in m³ between existing ground levels and formation levels on a square grid, one square at a
    time: plan area times the mean of the four corner depths where all four cut or all fill; where a square crosses
    the formation, area × (sum of cut depths)² / (4 × sum of all depths) for the cut and the same for the fill."""
    rows, columns = len(levels), len(levels[0]) if levels else 0
    if rows < 2 or columns < 2 or any(len(r) != columns for r in levels):
        raise ValueError("Give the levels as rows of the same length, at least 2 by 2.")
    if len(formation) != rows or any(len(r) != columns for r in formation):
        raise ValueError("Give the formation levels on the same grid as the ground levels.")
    if spacing <= 0:
        raise ValueError("The grid spacing must be more than zero.")
    area, cut, fill = spacing * spacing, Decimal(0), Decimal(0)
    for r in range(rows - 1):
        for c in range(columns - 1):
            corners = [(r, c), (r, c + 1), (r + 1, c), (r + 1, c + 1)]
            depths = [levels[i][j] - formation[i][j] for i, j in corners]
            up, down = sum(d for d in depths if d > 0), -sum(d for d in depths if d < 0)
            if not down:
                cut += area * up / 4
            elif not up:
                fill += area * down / 4
            else:
                cut += area * up * up / (4 * (up + down))
                fill += area * down * down / (4 * (up + down))
    return cut, fill, (rows - 1) * (columns - 1)
