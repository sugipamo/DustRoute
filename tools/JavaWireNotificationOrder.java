import java.util.HashSet;

/** Java 21 collection-order oracle, not a Minecraft server observation.
 * Cell.hashCode is the audited Vec3i implementation; the HashSet is the actual
 * JDK implementation used by DefaultRedstoneController's seven-position set.
 * Run: java tools/JavaWireNotificationOrder.java > <new-fixture-path>
 */
class JavaWireNotificationOrder {
    record Cell(int x, int y, int z) {
        @Override public int hashCode() { return (y + z * 31) * 31 + x; }
        String json() { return "{\"x\":" + x + ",\"y\":" + y + ",\"z\":" + z + "}"; }
    }
    public static void main(String[] args) {
        Cell[] origins = {
            new Cell(0, 4, 0), new Cell(1, 4, 0), new Cell(4, 4, 0), new Cell(-20, -2, 33),
            new Cell(32000, 184, 1000), new Cell(32001, 184, 1000),
            new Cell(29999990, 255, -29999990)
        };
        int[][] directions = {{0,-1,0},{0,1,0},{0,0,-1},{0,0,1},{-1,0,0},{1,0,0}};
        for (Cell origin : origins) {
            var positions = new HashSet<Cell>();
            positions.add(origin);
            for (int[] d : directions) positions.add(new Cell(origin.x+d[0], origin.y+d[1], origin.z+d[2]));
            String ordered = positions.stream().map(Cell::json).collect(java.util.stream.Collectors.joining(","));
            System.out.println("{\"origin\":"+origin.json()+",\"centers\":["+ordered+"]}");
        }
    }
}
