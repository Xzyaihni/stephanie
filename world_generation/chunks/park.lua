used_tiles = {"grass"}

function generate(neighbors)
    local chunk = {}

    for i = 1, chunk_size * chunk_size * chunk_size
    do
        chunk[i] = tilemap["grass"]
    end

    return chunk
end