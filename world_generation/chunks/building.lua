function(neighbors)
    local chunk = {};

    for i = 1, 16 * 16 do
        local tile = "grass";

        if i % 2 == 0 then
            tile = "concrete";
        end

        chunk[i] = tilemap[tile];
    end

    return chunk;
end
