(define (generate-chunk middle-position part)

(define building-height
    (let ((x (assq 'building-height (chunk-tags-at middle-position))))
        (if debug-mode
            (if (null? x)
                (begin
                    (if (not (allow-out-of-range-chunks)) (begin (display "building-height not found") (newline)))
                    19)
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

(define (light-intensity x) (if (stop-between-difficulty 2.0 4.0) x 0.0))

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
            (tile 'ceramic-tiles))
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
        (load "interior_common.scm")
        (put-outer-walls this-chunk)
        (big-put-tile this-chunk (make-point 5 2) wall-tile)
        (big-vertical-line this-chunk (make-point 10 2) 4 wall-tile)
        (big-horizontal-line this-chunk (make-point 5 4) 2 wall-tile)
        (big-horizontal-line this-chunk (make-point 11 5) 2 wall-tile)
        (big-vertical-line this-chunk (make-point 5 5) 2 wall-tile)
        (big-horizontal-line this-chunk (make-point 5 8) 9 wall-tile)
        (big-put-tile this-chunk (make-point 9 4) wall-tile)
        (big-vertical-line this-chunk (make-point 8 9) 5 wall-tile)
        (big-put-tile this-chunk (make-point 5 3) (single-marker (list 'door side-up 'wood 1)))
        (big-put-tile this-chunk (make-point 7 4) (single-marker (list 'door side-left 'wood 2)))
        (big-put-tile this-chunk (make-point 13 5) (single-marker (list 'door side-left 'wood 1)))
        (big-put-tile this-chunk (make-point 5 7) (single-marker (list 'door side-up 'wood 1)))
        (big-put-tile this-chunk (make-point 12 8) (single-marker (list 'door side-left 'wood 1)))
        (big-horizontal-line this-chunk (make-point 9 14) 5 (tile 'glass))
        (big-horizontal-line this-chunk (make-point 2 14) 5 (tile 'glass))
        (big-vertical-line this-chunk (make-point 1 9) 5 (tile 'glass))
        (big-vertical-line this-chunk (make-point 1 2) 5 (tile 'glass))
        (big-vertical-line this-chunk (make-point 14 9) 5 (tile 'glass))
        (big-vertical-line this-chunk (make-point 14 2) 5 (tile 'glass))
        (big-put-tile this-chunk (make-point 7 2) (single-marker (list 'light (light-intensity 0.7) '(0.5 0.5 0.0))))
        (big-put-tile this-chunk (make-point 7 6) (single-marker (list 'light (light-intensity 0.8) '(0.5 0.0 0.0))))
        (big-put-tile this-chunk (make-point 11 6) (single-marker (list 'light (light-intensity 0.7) '(0.5 0.5 0.0))))
        (big-put-tile this-chunk (make-point 12 3) (single-marker (list 'light (light-intensity 0.5) '(0.0 0.0 0.0))))
        (big-put-tile this-chunk (make-point 11 11) (single-marker (list 'light (light-intensity 0.8) '(0.0 0.5 0.0))))
        (big-put-tile this-chunk (make-point 4 11) (single-marker (list 'light (light-intensity 1.0) '(0.5 0.0 0.0))))
        (big-put-tile this-chunk (make-point 3 4) (single-marker (list 'light (light-intensity 0.8) '(0.5 0.0 0.0))))
        (try-put-furniture (make-point 11 11) (list 'furniture 'wood_table side-up))
        (try-put-furniture (make-point 10 11) (chair-list side-left))
        (try-put-furniture (make-point 10 12) (chair-list side-left))
        (try-put-furniture (make-point 12 11) (chair-list side-right))
        (try-put-furniture (make-point 12 12) (chair-list side-right))
        (try-put-furniture (make-point 11 10) (chair-list side-up))
        (try-put-furniture (make-point 11 13) (chair-list side-down))
        (try-put-furniture (make-point 7 6) (list 'furniture 'wood_table side-left))
        (try-put-furniture (make-point (if (random-bool-seeded (seed-with furnitures-seed 222)) 7 8) 7) (chair-list side-down))
        (try-put-furniture (make-point 13 7) (potted-plant-list side-right))
        (if (random-bool-seeded (seed-with furnitures-seed 333))
            (begin
                (try-put-furniture (make-point 7 9) (potted-plant-list side-right))
                (try-put-furniture (make-point 7 13) (potted-plant-list side-down))
                (try-put-furniture (make-point 5 9) (list 'furniture 'wood_table side-left))
                (try-put-furniture (make-point 5 13) (list 'furniture 'wood_table side-left))
                (try-put-furniture (make-point 2 12) (list 'furniture 'wood_table side-down))
                (try-put-furniture (make-point 2 4) (list 'furniture 'wood_table side-down))
                (try-put-furniture (make-point 2 8) (list 'furniture 'wood_table side-down))
                (try-put-furniture (make-point 3 (if (random-bool) 3 4)) (chair-list side-right))
                (try-put-furniture (make-point 3 (if (random-bool) 11 12)) (chair-list side-right))
                (try-put-furniture (make-point (if (random-bool) 5 6) 10) (chair-list side-down))
                (try-put-furniture (make-point (if (random-bool) 5 6) 12) (chair-list side-up))
                (try-put-furniture (make-point 3 (if (random-bool-seeded (seed-with furnitures-seed 23123)) 7 8)) (chair-list side-right)))
            (begin
                (if (difficulty-chance 0.1 0.0) (try-put-furniture (make-point 3 4) (list 'enemy 'office_zob)))
                (if (difficulty-chance 0.1 0.0) (try-put-furniture (make-point 5 10) (list 'enemy 'office_zob)))
                (if (difficulty-chance 0.1 0.0) (try-put-furniture (make-point 3 7) (list 'enemy 'office_zob)))
                (try-put-furniture (make-point 4 2) (potted-plant-list side-right))
                (try-put-furniture (make-point 4 5) (potted-plant-list side-right))
                (try-put-furniture (make-point 5 9) (potted-plant-list side-up))
                (try-put-furniture (make-point 7 9) (list 'furniture 'wood_table side-up))
                (try-put-furniture (make-point 7 12) (list 'furniture 'wood_table side-up))
                (try-put-furniture (make-point 2 3) (list 'furniture 'wood_table side-down))
                (try-put-furniture (make-point 2 6) (list 'furniture 'wood_table side-down))
                (try-put-furniture (make-point 2 10) (list 'furniture 'wood_table side-down))
                (try-put-furniture (make-point 2 13) (list 'furniture 'wood_table side-down))
                (try-put-furniture (make-point 6 (if (random-bool) 9 10)) (chair-list side-left))
                (try-put-furniture (make-point 6 (if (random-bool) 12 13)) (chair-list side-left))
                (try-put-furniture (make-point 3 (if (random-bool) 9 10)) (chair-list side-right))
                (try-put-furniture (make-point 3 (if (random-bool) 12 13)) (chair-list side-right))
                (try-put-furniture (make-point 3 (if (random-bool) 2 3)) (chair-list side-right))
                (try-put-furniture (make-point 3 (if (random-bool) 5 6)) (chair-list side-right))))
        (if (difficulty-chance 0.1 0.0) (try-put-furniture (make-point 4 10) (list 'enemy 'office_zob)))
        (if (difficulty-chance 0.1 0.0) (try-put-furniture (make-point 5 11) (list 'enemy 'office_zob)))
        (if (difficulty-chance 0.1 0.0) (try-put-furniture (make-point 4 4) (list 'enemy 'office_zob)))
        (try-put-furniture (make-point 4 11) (list 'enemy 'office_zob))
        (try-put-furniture (make-point 9 7) (list 'enemy 'office_zob))
        (if (difficulty-chance 0.1 0.0)
            (begin
                (try-put-furniture (make-point 9 9) (list 'enemy 'office_zob))
                (try-put-furniture (make-point 10 13) (list 'enemy 'office_zob))
                (try-put-furniture (make-point 12 13) (list 'enemy 'office_zob))
                (try-put-furniture (make-point 10 10) (list 'enemy 'office_zob))
                (try-put-furniture (make-point 12 10) (list 'enemy 'office_zob))))
        (generate-bathroom-generic
            (seed-with furnitures-seed 11111)
            (list
                (cons side-up (make-area (make-point 12 2) (make-point 2 1)))
                (cons side-left (make-area (make-point 11 2) (make-point 1 3)))
                (cons side-down (make-area (make-point 12 4) (make-point 1 1))))
            (make-area (make-point 12 3) (make-point 1 1))
            #t)
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
