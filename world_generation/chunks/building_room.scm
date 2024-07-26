(define (residential-building)
    (define this-chunk (filled-chunk (tile 'air)))

    (define (this-tile point tile) (put-tile this-chunk point tile))

    (define wall-material (tile 'concrete))

    ; outer walls
    (vertical-line-length
        this-chunk
        (make-point 1 1)
        (- size-y 2)
        wall-material)

    (vertical-line-length
        this-chunk
        (make-point (- size-x 2) 1)
        (- size-y 2)
        wall-material)

    (horizontal-line-length
        this-chunk
        (make-point 1 1)
        (- size-x 2)
        wall-material)

    (horizontal-line-length
        this-chunk
        (make-point 1 (- size-y 2))
        (- size-x 2)
        wall-material)

    ; hallway
    (fill-area
        this-chunk
        (make-point 6 0)
        (make-point 4 (- size-x 1))
        wall-material)

    ; stairwell
    (fill-area
        this-chunk
        (make-point 5 0)
        (make-point 6 4)
        wall-material)

    (fill-area
        this-chunk
        (make-point 6 1)
        (make-point 4 2)
        (tile 'air))

    (fill-area
        this-chunk
        (make-point 7 1)
        (make-point 2 (- size-y 3))
        (tile 'air))

    (define (door x)
        (this-tile
            (make-point x 12)
            (tile 'air)))

    (door 6)
    (door 9)

    (define (add-windows x)
        (this-tile
            (make-point x 3)
            (tile 'glass))
        (this-tile
            (make-point x (- size-y 4))
            (tile 'glass)))

    (add-windows 1)
    (add-windows (- size-x 2))

    this-chunk)

(define this-chunk (residential-building))

(let ((x (if (= (remainder height 4) 3) 6 9)))
    (put-tile
        this-chunk
        (make-point x 1)
        (tile 'stairs_up)))

(if (= height 1)
    ; entrance
    (horizontal-line-length
        this-chunk
        (make-point 7 0)
        2
        (tile 'air))
    this-chunk)
