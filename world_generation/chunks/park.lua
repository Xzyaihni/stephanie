function(neighbors)
    local chunk = {}

    local chunk_local = 16 * 16 * 1;
    for i = 1, chunk_local, 1 do
        chunk[i] = tilemap["grass"]
    end

    return chunk
end