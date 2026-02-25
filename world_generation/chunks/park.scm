(if (= height 1)
    (begin
        (define this-chunk (filled-chunk (tile 'air)))

        (define wall-tile (tile 'concrete))

        (define (wall-hole wall-x wall-y)
            (if (random-bool)
                (put-tile this-chunk (make-point wall-x (random-integer-between 1 (- size-y 1))) (tile 'air))
                (put-tile this-chunk (make-point (random-integer-between 1 (- size-x 1)) wall-y) (tile 'air))))

        (define (isnt-park position)
            (let
                ((chunk (car (chunk-at position))))
                (not (or (eq? chunk 'park) (eq? chunk 'bunker)))))

        (define (isnt-park-side chunk-side)
            (isnt-park (position-at-side position chunk-side)))

        (let
            (
                (isnt-up-park (isnt-park-side side-up))
                (isnt-left-park (isnt-park-side side-left))
                (isnt-right-park (isnt-park-side side-right))
                (isnt-down-park (isnt-park-side side-down)))
            (begin
                (if isnt-up-park (horizontal-line this-chunk 0 wall-tile))
                (if isnt-down-park (horizontal-line this-chunk (- size-y 1) wall-tile))
                (if isnt-left-park (vertical-line this-chunk 0 wall-tile))
                (if isnt-right-park (vertical-line this-chunk (- size-x 1) wall-tile))

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
                            (if (and isnt-right-park isnt-down-park) (wall-hole (- size-x 1) (- size-y 1))))))))

        this-chunk)
    (filled-chunk (tile 'grassie)))
