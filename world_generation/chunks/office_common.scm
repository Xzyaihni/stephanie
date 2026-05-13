(define (generate-chunk middle-position part)

(define building-height
    (let ((x (assq 'building-height (chunk-tags-at middle-position))))
        (if debug-mode
            (if (null? x)
                (begin
                    (if (not (allow-out-of-range-chunks)) (begin (display "building-height not found") (newline)))
                    15)
                x)
            x)))

(if (>= height building-height) (filled-chunk (tile 'air))
(begin

(define big-size-x (* size-x 2))
(define big-size-y (* size-y 2))

(define in-big-chunk-pos
    (point-zip-map
        (make-point size-x size-y)
        (cond
            ((eq? part 'bl) (make-point 0 1))
            ((eq? part 'br) (make-point 1 1))
            ((eq? part 'tl) (make-point 0 0))
            (else (make-point 1 0)))
        (lambda (x y) (* x y))))

(load "multichunk_common.scm")

(define wall-tile (tile 'concrete))

(define (put-outer-walls this-chunk)
    (big-vertical-line
        (big-horizontal-line
            (big-vertical-line
                (big-horizontal-line
                    this-chunk
                    (make-point 1 1)
                    14
                    wall-tile)
                (make-point 1 2)
                13
                wall-tile)
            (make-point 2 14)
            13
            wall-tile)
        (make-point 14 2)
        12
        wall-tile))

(define (put-floor this-chunk)
    (big-fill-area
        (big-fill-area
            (put-outer-walls this-chunk)
            (make-area (make-point 2 2) (make-point (- big-size-x 4) (- big-size-y 4)))
            (tile 'wood))
        (make-area (make-point 5 1) (make-point 6 4))
        (tile 'concrete)))

(define roof-start (- building-height 4))

(cond
    ((> height roof-start)
        (cond
            ((= height (+ roof-start 1))
                (define this-chunk
                    (big-fill-area
                        (filled-chunk (tile 'air))
                        (make-area (make-point 1 1) (make-point 14 14))
                        (tile 'concrete)))
                (big-put-tile
                    this-chunk
                    (make-point 6 2)
                    (tile 'stairs-down rotation))
                this-chunk)
            ((= height (+ roof-start 2))
                (define this-chunk (filled-chunk (tile 'air)))
                (define fence 'concrete-fence)
                (let
                    (
                        (locked-rotation-a (cond ((= rotation side-right) side-down) ((= rotation side-left) side-up) (else rotation)))
                        (locked-rotation-b (cond ((= rotation side-right) side-up) ((= rotation side-left) side-down) (else rotation))))
                    (begin
                        (big-vertical-line this-chunk (make-point 1 2) 12 (tile fence (side-combine locked-rotation-b side-up)))
                        (big-horizontal-line this-chunk (make-point 12 1) 2 (tile fence (side-combine locked-rotation-a side-up)))
                        (big-horizontal-line this-chunk (make-point 2 1) 2 (tile fence (side-combine locked-rotation-a side-up)))
                        (big-horizontal-line this-chunk (make-point 2 14) 12 (tile fence (side-combine locked-rotation-a side-down)))
                        (big-vertical-line this-chunk (make-point 14 2) 12 (tile fence (side-combine locked-rotation-b side-down)))))
                (big-put-tile this-chunk (make-point 14 14) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 1) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 14) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 14 1) (tile 'concrete-fence))
                (big-horizontal-line this-chunk (make-point 4 1) 8 wall-tile)
                (big-vertical-line this-chunk (make-point 4 2) 3 wall-tile)
                (big-vertical-line this-chunk (make-point 11 2) 3 wall-tile)
                (big-horizontal-line this-chunk (make-point 5 4) 4 wall-tile)
                (big-put-tile this-chunk (make-point 7 2) (single-marker (list 'light (light-intensity 0.7) '(0.5 0.0 0.0))))
                (big-put-tile this-chunk (make-point 9 4) (single-marker (list 'door side-left 'metal 2))))
            ((= height (+ roof-start 3))
                (big-fill-area (filled-chunk (tile 'air)) (make-area (make-point 4 1) (make-point 8 4)) wall-tile))))
    ((= height 0)
        (put-floor (filled-chunk (tile 'concrete-path))))
    ((= (remainder height 2) 0)
        (define this-chunk (put-floor (filled-chunk (tile 'air))))
        (let ((x (if (= (remainder height 4) 0) 6 9)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-down rotation)))
        this-chunk)
    (else
        (define furnitures-seed
            (seed-with
                (seed-with
                    (let ((x (assq 'building-seed (chunk-tags-at middle-position))))
                        (if debug-mode (if (null? x) (begin (display "building-seed not found") (newline) 0) x) x))
                    height)
                2222))

        (define this-chunk (filled-chunk (tile 'air)))
        (define (decide-enemy type)
            (if (eq? type 'normal)
                (pick-weighted 'zob 'runner 0.25)
                'bigy))
        (load "interior_common.scm")
        (put-outer-walls this-chunk)
        (big-put-tile this-chunk (make-point 5 2) wall-tile)
        (big-vertical-line this-chunk (make-point 10 2) 4 wall-tile)
        (big-horizontal-line this-chunk (make-point 5 4) 2 wall-tile)
        (big-horizontal-line this-chunk (make-point 11 5) 2 wall-tile)
        (big-vertical-line this-chunk (make-point 5 5) 3 wall-tile)
        (big-horizontal-line this-chunk (make-point 5 9) 9 wall-tile)
        (big-put-tile this-chunk (make-point 9 4) wall-tile)
        (big-vertical-line this-chunk (make-point 8 10) 4 wall-tile)
        (big-put-tile this-chunk (make-point 5 3) (single-marker (list 'door side-up 'wood 1)))
        (big-put-tile this-chunk (make-point 7 4) (single-marker (list 'door side-left 'wood 2)))
        (big-put-tile this-chunk (make-point 13 5) (single-marker (list 'door side-left 'wood 1)))
        (big-put-tile this-chunk (make-point 5 8) (single-marker (list 'door side-up 'wood 1)))
        (big-put-tile this-chunk (make-point 12 9) (single-marker (list 'door side-left 'wood 1)))
        (big-horizontal-line this-chunk (make-point 10 14) 3 (tile 'glass))
        (try-put-furniture (make-point 11 11) (list 'furniture 'wood_table side-up))
        (generate-bathroom
            (seed-with furnitures-seed 11111)
            (list
                (cons side-up (make-area (make-point 11 2) (make-point 3 1)))
                (cons side-left (make-area (make-point 11 3) (make-point 1 2)))
                (cons side-down (make-area (make-point 12 4) (make-point 1 1)))
                (cons side-right (make-area (make-point 13 3) (make-point 1 1))))
            (make-area (make-point 12 3) (make-point 1 1)))
        (if (= height 1)
            (begin
                (big-put-tile this-chunk (make-point 7 1) (tile 'air))
                (big-put-tile this-chunk (make-point 8 1) (tile 'air))
                (big-put-tile this-chunk (make-point 7 1) (single-marker (list 'door side-left 'metal 2)))))
        (let ((x (if (= (remainder height 4) 3) 6 9)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-up rotation)))
        this-chunk))

))

)
