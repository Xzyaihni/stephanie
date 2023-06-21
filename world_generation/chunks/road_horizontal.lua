function(neighbors)
    local chunk = {};

    local border_width = 3;

    local lines = {};
    for i = 1, 16 do
        local line = "asphalt";

        if (i < 1 + border_width) or (i > 16 - border_width) then
            line = "concrete";
        end

        lines[i] = line;
    end

    for y = 0, 15 do
        local line = lines[y + 1];

        for x = 0, 15 do
            local i = y * 16 + x + 1;

            chunk[i] = tilemap[line];
        end
    end

    return chunk;
end
