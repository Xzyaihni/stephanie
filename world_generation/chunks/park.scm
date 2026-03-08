(if (= height 1)
    (begin
        (define (make-park-walls this-chunk)
            (define wall-tile (tile 'metal-fence))

            (define (wall-hole wall-x wall-y)
                (if (random-bool)
                    (put-tile this-chunk (make-point wall-x (if (random-bool) 2 (- size-y 3))) (tile 'air))
                    (put-tile this-chunk (make-point (if (random-bool) 2 (- size-x 3)) wall-y) (tile 'air))))

            (define (isnt-park position)
                (let
                    ((chunk (car (chunk-at position))))
                    (not (or (eq? chunk 'park) (eq? chunk 'bunker)))))

            (define (isnt-park-side chunk-side)
                (isnt-park (position-at-side position chunk-side)))

            (let
                (
                    (bench-side (random-integer 4))
                    (isnt-up-park (isnt-park-side side-up))
                    (isnt-left-park (isnt-park-side side-left))
                    (isnt-right-park (isnt-park-side side-right))
                    (isnt-down-park (isnt-park-side side-down)))
                (begin
                    (if isnt-up-park
                        (begin
                            (horizontal-line this-chunk 0 wall-tile)
                            (if (= bench-side side-up) (combine-markers
                                this-chunk
                                (make-point 3 1)
                                (list 'furniture 'bench side-up '(0.5 0.0 0.0))))))

                    (if isnt-down-park
                        (begin
                            (horizontal-line this-chunk (- size-y 1) wall-tile)
                            (if (= bench-side side-down) (combine-markers
                                this-chunk
                                (make-point 4 (- size-y 2))
                                (list 'furniture 'bench side-down '(0.5 0.0 0.0))))))

                    (if isnt-left-park
                        (begin
                            (vertical-line this-chunk 0 wall-tile)
                            (if (= bench-side side-right) (combine-markers
                                this-chunk
                                (make-point 1 3)
                                (list 'furniture 'bench side-left)))))

                    (if isnt-right-park
                        (begin
                            (vertical-line this-chunk (- size-x 1) wall-tile)
                            (if (= bench-side side-left) (combine-markers
                                this-chunk
                                (make-point (- size-x 2) 3)
                                (list 'furniture 'bench side-right)))))

                    (if (isnt-park (position-at-side (position-at-side position side-up) side-left))
                        (put-tile this-chunk (make-point 0 0) wall-tile))

                    (if (isnt-park (position-at-side (position-at-side position side-up) side-right))
                        (put-tile this-chunk (make-point (- size-x 1) 0) wall-tile))

                    (if (isnt-park (position-at-side (position-at-side position side-down) side-left))
                        (put-tile this-chunk (make-point 0 (- size-y 1)) wall-tile))

                    (if (isnt-park (position-at-side (position-at-side position side-down) side-right))
                        (put-tile this-chunk (make-point (- size-x 1) (- size-y 1)) wall-tile))

                    (if (and isnt-left-park isnt-up-park)
                        (wall-hole 0 0)
                        (if (and isnt-right-park isnt-up-park)
                            (wall-hole (- size-x 1) 0)
                            (if (and isnt-left-park isnt-down-park)
                                (wall-hole 0 (- size-y 1))
                                (if (and isnt-right-park isnt-down-park) (wall-hole (- size-x 1) (- size-y 1)))))))))

        (define (make-park-grass this-chunk grass-area)
            (define (decide-enemy type)
                (cond
                    ((eq? type 'easy) (pick-weighted 'old 'smol 0.25))
                    ((eq? type 'normal) (pick-weighted 'zob 'runner 0.25))
                    (else 'bigy)))

            (define (place-enemy point)
                (let
                    ((enemy-type
                        (gradient-pick
                            '(none easy normal strong)
                            difficulty
                            1.0
                            3.0)))
                    (if (not (eq? enemy-type 'none))
                        (combine-markers
                            this-chunk
                            point
                            (list 'enemy (decide-enemy enemy-type))))))

            (define (try-spawn-grass-enemies this-chunk grass-area)
                (if (> (area-area grass-area) 4)
                    (let
                        ((halves (area-halves-x grass-area)))
                        (begin (try-spawn-grass-enemies this-chunk (car halves)) (try-spawn-grass-enemies this-chunk (cdr halves))))
                    (place-enemy (area-point-random grass-area))))

            (define grass-rate 0.25)

            (try-spawn-grass-enemies this-chunk grass-area)
            (for-each-tile
                (lambda (pos)
                    (if (< (random-float) grass-rate)
                        (combine-markers
                            this-chunk
                            pos
                            (list
                                'furniture
                                (if (= (random-integer 9) 0)
                                    (if (random-bool) 'bush1 'bush2)
                                    (if (random-bool) 'grass1 'grass2))
                                side-up))))
                grass-area))

        (define this-chunk (filled-chunk (tile 'air)))

        (make-park-walls this-chunk)

        (make-park-grass this-chunk (make-area (make-point 2 2) (make-point (- size-x 4) (- size-y 4))))

        this-chunk)
    (filled-chunk (tile 'grassie)))
