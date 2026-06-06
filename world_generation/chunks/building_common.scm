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

(define big-size-x (* size-x 3))
(define big-size-y (* size-y 3))

(define in-big-chunk-pos
    (point-zip-map
        (make-point size-x size-y)
        (cond
            ((eq? part 'bl) (make-point 0 2))
            ((eq? part 'b) (make-point 1 2))
            ((eq? part 'br) (make-point 2 2))
            ((eq? part 'l) (make-point 0 1))
            ((eq? part 'm) (make-point 1 1))
            ((eq? part 'r) (make-point 2 1))
            ((eq? part 'tl) (make-point 0 0))
            ((eq? part 't) (make-point 1 0))
            (else (make-point 2 0)))
        (lambda (x y) (* x y))))

(load "multichunk_common.scm")

(define building-difficulty (difficulty-at middle-position))

(define (light-intensity x) (if (stop-between-difficulty-with building-difficulty 0.5 2.0) x 0.0))

(define wall-tile (tile 'concrete))

(define (put-outer-walls this-chunk)
    (big-horizontal-line
        (big-horizontal-line
            (big-vertical-line
                (big-vertical-line
                    (big-horizontal-line
                        (big-horizontal-line
                            this-chunk
                            (make-point 1 1)
                            7
                            wall-tile)
                        (make-point 16 1)
                        7
                        wall-tile)
                    (make-point 1 2)
                    21
                    wall-tile)
                (make-point 22 2)
                21
                wall-tile)
            (make-point 2 22)
            20
            wall-tile)
        (make-point 7 0)
        10
        wall-tile))

(define (put-floor this-chunk)
    (big-fill-area
        (big-fill-area
            (put-outer-walls this-chunk)
            (make-area (make-point 2 2) (make-point (- big-size-x 4) (- big-size-y 4)))
            (tile 'wood))
        (make-area (make-point 8 1) (make-point size-x 4))
        (tile 'concrete)))

(define roof-start (- building-height 4))

(cond
    ((> height roof-start)
        (cond
            ((= height (+ roof-start 1))
                (define this-chunk
                    (big-horizontal-line
                        (big-fill-area
                            (filled-chunk (tile 'air))
                            (make-area (make-point 1 1) (make-point 22 22))
                            (tile 'concrete))
                        (make-point 7 0)
                        10
                        (tile 'concrete)))
                (big-put-tile
                    this-chunk
                    (make-point 9 2)
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
                        (big-vertical-line this-chunk (make-point 1 2) 20 (tile fence (side-combine locked-rotation-b side-up)))
                        (big-horizontal-line this-chunk (make-point 17 1) 5 (tile fence (side-combine locked-rotation-a side-up)))
                        (big-horizontal-line this-chunk (make-point 2 1) 5 (tile fence (side-combine locked-rotation-a side-up)))
                        (big-horizontal-line this-chunk (make-point 2 22) 20 (tile fence (side-combine locked-rotation-a side-down)))
                        (big-vertical-line this-chunk (make-point 22 2) 20 (tile fence (side-combine locked-rotation-b side-down)))))
                (big-put-tile this-chunk (make-point 22 22) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 1) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 1 22) (tile 'concrete-fence))
                (big-put-tile this-chunk (make-point 22 1) (tile 'concrete-fence))
                (big-horizontal-line this-chunk (make-point 7 0) 10 wall-tile)
                (big-vertical-line this-chunk (make-point 7 1) 4 wall-tile)
                (big-vertical-line this-chunk (make-point 16 1) 4 wall-tile)
                (big-horizontal-line this-chunk (make-point 8 4) 6 wall-tile)
                (big-put-tile this-chunk (make-point 9 2) (single-marker (list 'light (light-intensity 0.7) '(0.0 0.0 0.0))))
                (big-put-tile this-chunk (make-point 14 2) (single-marker (list 'light (light-intensity 0.7) '(0.5 0.0 0.0))))
                (big-put-tile this-chunk (make-point 14 4) (single-marker (list 'door side-left 'metal 2))))
            ((= height (+ roof-start 3))
                (big-fill-area (filled-chunk (tile 'air)) (make-area (make-point 7 0) (make-point 10 5)) wall-tile))))
    ((= height 0)
        (put-floor (filled-chunk (tile 'concrete-path))))
    ((= (remainder height 2) 0)
        (define this-chunk (put-floor (filled-chunk (tile 'air))))
        (let ((x (if (= (remainder height 4) 0) 9 14)))
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
        (define (generate-bathroom room-seed wall-areas middle-area) (generate-bathroom-generic room-seed wall-areas middle-area #f))
        (define (generate-main-room room-seed wall-areas middle-area)
            (define skip-tracker (cons 'tracker 0))
            (define (skip-one)
                (if (= (cdr skip-tracker) 0)
                    (begin (set-cdr! skip-tracker 1) #f)
                    #t))
            (define (reset-skip-one) (set-cdr! skip-tracker 0))
            (define counter (cons 'counter 0))
            (define (is-nth n)
                (set-cdr! counter (+ 1 (cdr counter)))
                (= n (cdr counter)))
            (define fits-vertical (not (< (point-y (area-size middle-area)) 3)))
            (define vertical-table
                (if (< (point-x (area-size middle-area)) 3)
                    (if fits-vertical #t '())
                    (if fits-vertical (random-bool-seeded (seed-with room-seed 8)) #f)))
            ;(mark-room-areas wall-areas '() (tile 'soil))
            (generate-room-with-furniture
                (seed-with room-seed 12)
                wall-areas
                (list
                    (lambda (inside-index outer-side current-area)
                        (if (= inside-index 0) (reset-skip-one))
                        (if (and (> (- (area-area current-area) inside-index) 2) (if (= outer-side side-left) (> inside-index 0) #t))
                            (let
                                (
                                    (put-it
                                        (lambda ()
                                            (try-put-furniture
                                                (area-index current-area inside-index)
                                                (list
                                                    'furniture
                                                    'bed
                                                    (side-combine outer-side side-right)))
                                            #t)))
                                (if (or (= outer-side side-right) (= outer-side side-up))
                                    (if (skip-one)
                                        (put-it)
                                        #f)
                                    (put-it)))
                            #f))
                    (lambda (a b c) #t)
                    (lambda (a b c) #t)
                    (lambda (a b c) #t)
                    (lambda (inside-index outer-side current-area)
                        (try-put-furniture
                            (area-index current-area inside-index)
                            (potted-plant-list outer-side))
                        #t)
                    (if (= (random-integer-seeded (seed-with room-seed 876) 5) 0)
                        (lambda (inside-index outer-side current-area)
                            (try-put-furniture
                                (area-index current-area inside-index)
                                (list
                                    'furniture
                                    'safe
                                    outer-side))
                            #t)
                        (lambda (a b c) #t))))
            (if (and (not (null? middle-area)) (> (area-area middle-area) 4))
                (generate-room-with-furniture
                    (seed-with room-seed 7)
                    (list (cons side-up middle-area))
                    (filter (lambda (x) (not (null? x))) (list
                        (lambda (inside-index outer-side current-area)
                            (try-put-furniture
                                (area-index current-area inside-index)
                                (list
                                    'enemy
                                    (decide-enemy
                                        (gradient-pick
                                            '(normal strong)
                                            building-difficulty
                                            0.2
                                            2.0))))
                            #t)
                        (if (null? vertical-table)
                            '()
                            (lambda (inside-index outer-side current-area)
                                (let
                                    (
                                        (pos (area-index current-area inside-index))
                                        (size (area-size current-area)))
                                    (let
                                        ((abs-pos (index-to-pos (point-x size) inside-index)))
                                        (if
                                            (if vertical-table
                                                (< (point-y abs-pos) (- (point-y size) 2))
                                                (< (point-x abs-pos) (- (point-x size) 2)))
                                            (begin
                                                (try-put-furniture
                                                    (area-index current-area inside-index)
                                                    (chair-list (if vertical-table side-up side-left)))
                                                #t)
                                            #f)))))
                        (if vertical-table
                            (lambda (a b c) (is-nth (- (point-x (area-size middle-area)) 1)))
                            '())
                        (if (null? vertical-table)
                            (lambda (a b c) #t)
                            (lambda (inside-index outer-side current-area)
                                (try-put-furniture
                                    (area-index current-area inside-index)
                                    (list
                                        'furniture
                                        'wood_table
                                        (if vertical-table side-up side-left)))
                                #t)))))))
        (define (generate-kitchen room-seed wall-areas middle-area)
            (define skip-tracker (cons 'tracker 0))
            (define (skip-one)
                (if (= (cdr skip-tracker) 0)
                    (begin (set-cdr! skip-tracker 1) #f)
                    #t))
            (define (reset-skip-one) (set-cdr! skip-tracker 0))
            ;(mark-room-areas wall-areas '() (tile 'grassie))
            (if (not (null? middle-area))
                (generate-room-with-furniture
                    (seed-with room-seed 387)
                    (list (cons side-up middle-area))
                    (list
                        (if (difficulty-chance-with building-difficulty 0.5 0.25)
                            (lambda (inside-index outer-side current-area)
                                (try-put-furniture
                                    (area-index current-area inside-index)
                                    (list
                                        'enemy
                                        (decide-enemy
                                            (gradient-pick
                                                '(normal strong)
                                                building-difficulty
                                                0.2
                                                2.0))))
                                #t)
                            (lambda (a b c) #t)))))
            (generate-room-with-furniture
                (seed-with room-seed 4)
                wall-areas
                (list
                    (lambda (inside-index outer-side current-area)
                        (if (> (- (area-area current-area) inside-index) 2)
                            (begin
                                (try-put-furniture
                                    (area-index current-area inside-index)
                                    (chair-list
                                        (side-combine
                                            outer-side
                                            (if (or (= outer-side side-left) (= outer-side side-down)) side-right side-left))))
                                #t)
                            #f))
                    (lambda (inside-index outer-side current-area)
                        (if (= inside-index 0) (reset-skip-one))
                        (let
                            (
                                (put-it
                                    (lambda ()
                                        (try-put-furniture
                                            (area-index current-area inside-index)
                                            (list
                                                'furniture
                                                'wood_table
                                                (side-combine outer-side side-left)))
                                        #t)))
                            (if (or (= outer-side side-left) (= outer-side side-down))
                                (if (skip-one)
                                    (put-it)
                                    #f)
                                (put-it)))))))
        (define (generate-side-room is-right)
            (define (flip-right s)
                (if is-right
                    (if (= s side-left) side-right side-left)
                    s))
            (define (side-offset-pos p)
                (if is-right (make-point (- (- big-size-x 1) (point-x p)) (point-y p)) p))
            (define (side-offset-area a)
                (if is-right
                    (let ((start (side-offset-pos (area-start a))) (size (area-size a)))
                        (make-area
                            (make-point (- (point-x start) (- (point-x size) 1)) (point-y start))
                            size))
                    a))
            (define (side-horizontal-line chunk p l tle)
                (big-horizontal-line this-chunk
                    (if is-right
                        (let ((start (side-offset-pos p))) (make-point (- (point-x start) (- l 1)) (point-y start)))
                        p)
                    l
                    tle))
            (let
                (
                    (variant (random-integer-seeded (seed-with furnitures-seed (if is-right 777 888)) 5))
                    (this-room-seed (seed-with (seed-with furnitures-seed 11111) (if is-right 222 111))))
                (cond
                    ((= variant 0)
                        (side-horizontal-line this-chunk (make-point 2 4) 5 wall-tile)
                        (side-horizontal-line this-chunk (make-point 2 12) 7 wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 6)) 5 (tile 'glass))
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 14)) 2 (tile 'glass))
                        (big-put-tile this-chunk (side-offset-pos (make-point 6 9)) (single-marker (list 'light (light-intensity 1.6) '(0.5 0.5 0.0))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 5 14)) (single-marker (list 'light (light-intensity 1.2))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 4 3)) (single-marker (list 'light (light-intensity 0.7) '(0.0 -0.5 0.0))))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point (if (random-bool-seeded this-room-seed) 3 4) 12))
                            (single-marker (list 'door side-right 'wood 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point (+ (random-integer-seeded this-room-seed 3) 3) 4))
                            (single-marker (list 'door side-right 'wood 1)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 13) (make-point 1 4))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 13) (make-point 1 4))))
                                (cons side-up (side-offset-area (make-area (make-point 5 13) (make-point 3 1))))
                                (cons side-down (side-offset-area (make-area (make-point 3 16) (make-point 5 1)))))
                            (side-offset-area (make-area (make-point 3 14) (make-point 5 2))))
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 2) (make-point 1 2))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 6 2) (make-point 1 2))))
                                (cons side-up (side-offset-area (make-area (make-point 3 2) (make-point 3 1)))))
                            '())
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 5) (make-point 1 7))))
                                (cons side-up (side-offset-area (make-area (make-point 6 5) (make-point 3 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 6) (make-point 1 2))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 9) (make-point 1 3))))
                                (cons side-down (side-offset-area (make-area (make-point 5 11) (make-point 3 1)))))
                            (side-offset-area (make-area (make-point 3 6) (make-point 5 5)))))
                    ((= variant 1)
                        (big-put-tile this-chunk (side-offset-pos (make-point 8 7)) wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 7 7)) 7 wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 5 9)) 4 wall-tile)
                        (side-horizontal-line this-chunk (make-point 2 9) 3 wall-tile)
                        (side-horizontal-line this-chunk (make-point 2 13) 4 wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 14)) 3 (tile 'glass))
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 2)) 7 (tile 'glass))
                        (big-put-tile this-chunk (side-offset-pos (make-point 6 15)) (single-marker (list 'light (light-intensity 1.4))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 4 5)) (single-marker (list 'light (light-intensity 1.2))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 6 11)) (single-marker (list 'light (light-intensity 0.7))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 3 11)) (single-marker (list 'light (light-intensity 0.6))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 8 10)) (single-marker (list 'light (light-intensity 0.7))))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 5 11))
                            (single-marker (list 'door side-up 'wood 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 6 9))
                            (single-marker (list 'door side-left 'wood 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 6 13))
                            (single-marker (list 'door side-left 'wood 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 8 13))
                            (single-marker (list 'door side-left 'wood 1)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 14) (make-point 1 3))))
                                (cons side-up (side-offset-area (make-area (make-point 3 14) (make-point 3 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 15) (make-point 1 2))))
                                (cons side-up (side-offset-area (make-area (make-point 7 14) (make-point 1 1))))
                                (cons side-down (side-offset-area (make-area (make-point 3 16) (make-point 5 1)))))
                            (side-offset-area (make-area (make-point 3 15) (make-point 5 1))))
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 10) (make-point 1 3))))
                                (cons side-up (side-offset-area (make-area (make-point 3 10) (make-point 2 1))))
                                (cons side-down (side-offset-area (make-area (make-point 3 12) (make-point 2 1)))))
                            (side-offset-area (make-area (make-point 3 11) (make-point 1 1))))
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 2) (make-point 1 7))))
                                (cons side-up (side-offset-area (make-area (make-point 3 2) (make-point 3 1))))
                                (cons side-down (side-offset-area (make-area (make-point 3 8) (make-point 3 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 6 2) (make-point 1 3))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 6 7) (make-point 1 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 5) (make-point 1 2))))
                                (cons side-up (side-offset-area (make-area (make-point 6 5) (make-point 2 1))))
                                (cons side-down (side-offset-area (make-area (make-point 6 6) (make-point 2 1)))))
                            (side-offset-area (make-area (make-point 3 3) (make-point 3 5)))))
                    ((= variant 2)
                        (big-put-tile this-chunk (side-offset-pos (make-point 2 7)) wall-tile)
                        (side-horizontal-line this-chunk (make-point 4 7) 5 wall-tile)
                        (side-horizontal-line this-chunk (make-point 4 4) 3 wall-tile)
                        (big-put-tile this-chunk (side-offset-pos (make-point 4 5)) wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 2)) 5 (tile 'glass))
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 9)) 3 (tile 'glass))
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 13)) 3 (tile 'glass))
                        (big-put-tile this-chunk (side-offset-pos (make-point 5 12)) (single-marker (list 'light (light-intensity 1.4))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 3 3)) (single-marker (list 'light (light-intensity 0.9))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 6 5)) (single-marker (list 'light (light-intensity 0.8) '(0.5 0.5 0.0))))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 4 6))
                            (single-marker (list 'door side-up 'wood 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 3 7))
                            (single-marker (list 'door side-left 'wood 1)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 2) (make-point 1 5))))
                                (cons side-up (side-offset-area (make-area (make-point 3 2) (make-point 4 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 3 4) (make-point 1 2))))
                                (cons side-down (side-offset-area (make-area (make-point 3 3) (make-point 4 1)))))
                            '())
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons side-up (side-offset-area (make-area (make-point 5 5) (make-point 4 1))))
                                (cons side-down (side-offset-area (make-area (make-point 6 6) (make-point 3 1)))))
                            '())
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 8) (make-point 1 9))))
                                (cons side-up (side-offset-area (make-area (make-point 4 8) (make-point 4 1))))
                                (cons side-down (side-offset-area (make-area (make-point 3 16) (make-point 6 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 9) (make-point 1 7)))))
                            (side-offset-area (make-area (make-point 3 9) (make-point 5 7)))))
                    ((= variant 3)
                        (big-put-tile this-chunk (side-offset-pos (make-point 8 10)) wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 6 4)) 4 wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 6 9)) 8 wall-tile)
                        (side-horizontal-line this-chunk (make-point 2 11) 3 wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 4)) 5 (tile 'glass))
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 13)) 3 (tile 'glass))
                        (big-put-tile this-chunk (side-offset-pos (make-point 7 13)) (single-marker (list 'light (light-intensity 0.9))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 7 8)) (single-marker (list 'light (light-intensity 0.9))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 4 3)) (single-marker (list 'light (light-intensity 0.8))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 3 7)) (single-marker (list 'light (light-intensity 0.9) '(0.5 0.0 0.0))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 3 14)) (single-marker (list 'light (light-intensity 0.9) '(0.5 0.0 0.0))))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 7 10))
                            (single-marker (list 'door side-left 'wood 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 5 11))
                            (single-marker (list 'door side-left 'wood 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 6 8))
                            (single-marker (list 'door side-up 'wood 1)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 12) (make-point 1 5))))
                                (cons side-up (side-offset-area (make-area (make-point 3 12) (make-point 2 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 5 13) (make-point 1 4))))
                                (cons side-down (side-offset-area (make-area (make-point 3 16) (make-point 2 1)))))
                            (side-offset-area (make-area (make-point 3 13) (make-point 2 3))))
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 7 12) (make-point 1 5))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 11) (make-point 1 6)))))
                            '())
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 2) (make-point 1 9))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 6 2) (make-point 1 2))))
                                (cons side-down (side-offset-area (make-area (make-point 3 10) (make-point 2 1))))
                                (cons side-up (side-offset-area (make-area (make-point 3 2) (make-point 3 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 5 9) (make-point 1 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 5 3) (make-point 1 5)))))
                            (side-offset-area (make-area (make-point 3 3) (make-point 2 7)))))
                    (else
                        (side-horizontal-line this-chunk (make-point 2 7) 7 wall-tile)
                        (side-horizontal-line this-chunk (make-point 2 13) 4 wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 5 14)) 3 wall-tile)
                        (big-vertical-line this-chunk (side-offset-pos (make-point 1 9)) 3 (tile 'glass))
                        (side-horizontal-line this-chunk (make-point 3 1) 3 (tile 'glass))
                        (big-put-tile this-chunk (side-offset-pos (make-point 6 11)) (single-marker (list 'light (light-intensity 1.4) '(0.5 0.0 0.0))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 3 15)) (single-marker (list 'light (light-intensity 0.6))))
                        (big-put-tile this-chunk (side-offset-pos (make-point 4 5)) (single-marker (list 'light (light-intensity 1.0) '(0.0 0.5 0.0))))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point 5 (if (random-bool-seeded this-room-seed) 15 16)))
                            (single-marker (list 'door side-down 'wood 1)))
                        (big-put-tile
                            this-chunk
                            (side-offset-pos (make-point (+ 3 (random-integer-seeded this-room-seed 5)) 7))
                            (single-marker (list 'door side-left 'wood 1)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 2) (make-point 1 5))))
                                (cons side-up (side-offset-area (make-area (make-point 3 2) (make-point 3 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 6 2) (make-point 1 4))))
                                (cons side-up (side-offset-area (make-area (make-point 7 5) (make-point 1 1))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 5) (make-point 1 2)))))
                            (side-offset-area (make-area (make-point 3 3) (make-point 3 3))))
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 14) (make-point 1 3))))
                                (cons side-up (side-offset-area (make-area (make-point 3 14) (make-point 2 1))))
                                (cons side-down (side-offset-area (make-area (make-point 3 16) (make-point 1 1)))))
                            (side-offset-area (make-area (make-point 3 15) (make-point 1 1))))
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 2 8) (make-point 1 5))))
                                (cons (flip-right side-right) (side-offset-area (make-area (make-point 8 9) (make-point 1 7))))
                                (cons side-down (side-offset-area (make-area (make-point 3 12) (make-point 3 1))))
                                (cons side-down (side-offset-area (make-area (make-point 7 16) (make-point 2 1))))
                                (cons (flip-right side-left) (side-offset-area (make-area (make-point 6 12) (make-point 1 3)))))
                            (side-offset-area (make-area (make-point 3 9) (make-point 5 3))))))))
        (define (generate-left-room) (generate-side-room #f))
        (define (generate-right-room) (generate-side-room #t))
        (define (generate-bottom-room)
            (let
                (
                    (variant (random-integer-seeded (seed-with furnitures-seed 999) 3))
                    (this-room-seed (seed-with (seed-with furnitures-seed 22222) 333)))
                (cond
                    ((= variant 0)
                        (big-vertical-line this-chunk (make-point 7 18) 4 wall-tile)
                        (big-vertical-line this-chunk (make-point 16 18) 4 wall-tile)
                        (big-horizontal-line this-chunk (make-point 9 22) 6 (tile 'glass))
                        (big-put-tile this-chunk (make-point 7 (if (random-bool) 19 20)) (single-marker (list 'door side-down 'wood 1)))
                        (big-put-tile this-chunk (make-point 16 (if (random-bool) 19 20)) (single-marker (list 'door side-down 'wood 1)))
                        (big-put-tile this-chunk (make-point 11 19) (single-marker (list 'light (light-intensity 1.0) '(0.5 0.0 0.0))))
                        (big-put-tile this-chunk (make-point 4 20) (single-marker (list 'light (light-intensity 0.8) '(0.5 0.0 0.0))))
                        (big-put-tile this-chunk (make-point 19 20) (single-marker (list 'light (light-intensity 0.8) '(0.5 0.0 0.0))))
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons side-down (make-area (make-point 8 21) (make-point 8 1)))
                                (cons side-up (make-area (make-point 8 18) (make-point 2 1)))
                                (cons side-up (make-area (make-point 14 18) (make-point 2 1)))
                                (cons side-up (make-area (make-point 10 17) (make-point 1 1)))
                                (cons side-up (make-area (make-point 13 17) (make-point 1 1))))
                            (make-area (make-point 9 19) (make-point 6 2)))
                        (let (
                            (room-calls
                                (if (random-bool-seeded this-room-seed)
                                    (cons generate-bathroom generate-kitchen)
                                    (cons generate-kitchen generate-bathroom))))
                            (begin
                                ((car room-calls)
                                    this-room-seed
                                    (list
                                        (cons side-up (make-area (make-point 2 18) (make-point 5 1)))
                                        (cons side-down (make-area (make-point 2 21) (make-point 5 1)))
                                        (cons side-left (make-area (make-point 2 19) (make-point 1 2))))
                                    (make-area (make-point 3 19) (make-point 3 2)))
                                ((cdr room-calls)
                                    this-room-seed
                                    (list
                                        (cons side-up (make-area (make-point 17 18) (make-point 5 1)))
                                        (cons side-down (make-area (make-point 17 21) (make-point 5 1)))
                                        (cons side-right (make-area (make-point 21 19) (make-point 1 2))))
                                    (make-area (make-point 18 19) (make-point 3 2))))))
                    ((= variant 1)
                        (big-vertical-line this-chunk (make-point 9 18) 3 wall-tile)
                        (big-horizontal-line this-chunk (make-point 10 20) 7 wall-tile)
                        (big-vertical-line this-chunk (make-point 17 18) 3 wall-tile)
                        (big-horizontal-line this-chunk (make-point 11 22) 5 (tile 'glass))
                        (big-put-tile this-chunk (make-point 12 18) (single-marker (list 'light (light-intensity 1.0))))
                        (big-put-tile this-chunk (make-point 11 21) (single-marker (list 'light (light-intensity 0.6))))
                        (big-put-tile this-chunk (make-point 14 21) (single-marker (list 'light (light-intensity 0.6))))
                        (big-put-tile this-chunk (make-point 5 18) (single-marker (list 'light (light-intensity 1.2))))
                        (big-put-tile this-chunk (make-point 19 19) (single-marker (list 'light (light-intensity 0.9) '(0.5 0.5 0.0))))
                        (big-put-tile this-chunk (make-point 9 21) (single-marker (list 'door side-down 'wood 1)))
                        (big-put-tile this-chunk (make-point 17 21) (single-marker (list 'door side-down 'wood 1)))
                        (let ((door-offset (random-integer-seeded this-room-seed 3)))
                            (begin
                                (big-put-tile
                                    this-chunk
                                    (make-point (+ door-offset 12) 20)
                                    (single-marker (list 'door side-left 'wood 1)))
                                (generate-main-room
                                    this-room-seed
                                    (list
                                        (cons side-right (make-area (make-point 13 17) (make-point 1 2)))
                                        (cons side-up (make-area (make-point 14 18) (make-point 3 1)))
                                        (cons side-down (make-area (make-point 10 19) (make-point (+ door-offset 2) 1)))
                                        (cons side-down (make-area (make-point (+ door-offset 13) 19) (make-point (- 4 door-offset) 1)))
                                        (cons side-left (make-area (make-point 10 17) (make-point 1 2))))
                                    (make-area (make-point 11 18) (make-point 2 1)))))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons side-left (make-area (make-point 2 18) (make-point 1 4)))
                                (cons side-up (make-area (make-point 3 18) (make-point 5 1)))
                                (cons side-down (make-area (make-point 3 21) (make-point 5 1)))
                                (cons side-right (make-area (make-point 8 18) (make-point 1 3))))
                            (make-area (make-point 3 19) (make-point 5 2)))
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons side-left (make-area (make-point 18 18) (make-point 1 3)))
                                (cons side-right (make-area (make-point 21 18) (make-point 1 4)))
                                (cons side-up (make-area (make-point 19 18) (make-point 2 1)))
                                (cons side-down (make-area (make-point 19 21) (make-point 2 1))))
                            (make-area (make-point 19 19) (make-point 2 2))))
                    (else
                        (big-vertical-line this-chunk (make-point 9 18) 3 wall-tile)
                        (big-horizontal-line this-chunk (make-point 10 19) 6 wall-tile)
                        (big-horizontal-line this-chunk (make-point 18 22) 2 (tile 'glass))
                        (big-horizontal-line this-chunk (make-point 11 22) 3 (tile 'glass))
                        (big-put-tile this-chunk (make-point 11 17) (single-marker (list 'light (light-intensity 0.7) '(0.5 0.5 0.0))))
                        (big-put-tile this-chunk (make-point 12 21) (single-marker (list 'light (light-intensity 0.7))))
                        (big-put-tile this-chunk (make-point 5 19) (single-marker (list 'light (light-intensity 1.0))))
                        (big-put-tile this-chunk (make-point 18 19) (single-marker (list 'light (light-intensity 1.0) '(0.5 0.5 0.0))))
                        (big-put-tile this-chunk (make-point 15 18) (single-marker (list 'door side-down 'wood 1)))
                        (big-put-tile this-chunk (make-point 9 21) (single-marker (list 'door side-down 'wood 1)))
                        (generate-main-room
                            this-room-seed
                            (list
                                (cons side-up (make-area (make-point 17 18) (make-point 4 1)))
                                (cons side-right (make-area (make-point 21 18) (make-point 1 4)))
                                (cons side-down (make-area (make-point 16 21) (make-point 5 1))))
                            (make-area (make-point 16 19) (make-point 5 2)))
                        (generate-kitchen
                            this-room-seed
                            (list
                                (cons side-up (make-area (make-point 10 20) (make-point 6 1)))
                                (cons side-down (make-area (make-point 11 21) (make-point 5 1))))
                            '())
                        (generate-bathroom
                            this-room-seed
                            (list
                                (cons side-left (make-area (make-point 2 18) (make-point 1 4)))
                                (cons side-right (make-area (make-point 8 18) (make-point 1 3)))
                                (cons side-down (make-area (make-point 3 21) (make-point 5 1)))
                                (cons side-up (make-area (make-point 3 18) (make-point 5 1))))
                            (make-area (make-point 3 19) (make-point 5 2)))))))
        (put-outer-walls this-chunk)
        (big-fill-area this-chunk (make-area (make-point 7 2) (make-point 1 3)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 16 2) (make-point 1 3)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 2 17) (make-point 8 1)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 14 17) (make-point 8 1)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 9 4) (make-point 1 13)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 14 4) (make-point 1 13)) wall-tile)
        (big-fill-area this-chunk (make-area (make-point 10 16) (make-point 4 1)) wall-tile)
        (if (= height 1)
            (begin
                (big-put-tile this-chunk (make-point 11 0) (tile 'air))
                (big-put-tile this-chunk (make-point 12 0) (tile 'air))
                (big-put-tile this-chunk (make-point 11 0) (single-marker (list 'door side-left 'metal 2)))))
        (big-put-tile this-chunk (make-point 8 4) (tile 'concrete))
        (big-put-tile this-chunk (make-point 15 4) (tile 'concrete))
        (big-put-tile this-chunk (make-point 11 5) (single-marker (list 'light (light-intensity 1.5) '(0.5 0.0 0.0))))
        (big-put-tile this-chunk (make-point 11 11) (single-marker (list 'light (light-intensity 1.5) '(0.5 0.0 0.0))))
        (big-put-tile this-chunk (make-point 11 2) (single-marker (list 'light (light-intensity 0.9) '(0.5 0.0 0.0))))
        (big-put-tile this-chunk (make-point 9 8) (single-marker (list 'door side-down 'metal 1)))
        (big-put-tile this-chunk (make-point 14 8) (single-marker (list 'door side-down 'metal 1)))
        (big-put-tile this-chunk (make-point (if (random-bool) 11 12) 16) (single-marker (list 'door side-right 'metal 1)))
        (let ((x (if (= (remainder height 4) 3) 9 14)))
            (big-put-tile
                this-chunk
                (make-point x 2)
                (tile 'stairs-up rotation)))
        (cond
            ((eq? part 'tr) (generate-right-room))
            ((eq? part 'r) (generate-right-room))
            ((eq? part 'tl) (generate-left-room))
            ((eq? part 'l) (generate-left-room))
            ((eq? part 't) (generate-left-room) (generate-right-room))
            ((eq? part 'm) (generate-left-room) (generate-right-room))
            ((eq? part 'bl) (generate-left-room) (generate-bottom-room))
            ((eq? part 'b) (generate-bottom-room) (generate-left-room) (generate-right-room))
            (else (generate-right-room) (generate-bottom-room)))
        this-chunk))

))

)
